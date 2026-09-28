//! Sealing and naming blobs.
//!
//! An encrypted repository has a random 32-byte master key. Two keys are derived from it: one
//! seals data (XChaCha20-Poly1305, a random 24-byte nonce per seal) and one keys the BLAKE3 hash
//! that names blobs. The master key is stored twice in `keepr-repo.json`, sealed by a key from
//! the password (Argon2id) and by one from the recovery key, so either opens the repository and
//! the password can change without rewriting anything else.
//!
//! An unencrypted repository still names blobs with a keyed hash (the key stored in the clear),
//! so the two kinds work the same way.

use crate::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

pub const NONCE: usize = 24;

pub struct Keys {
    seal: Option<XChaCha20Poly1305>,
    id_key: [u8; 32],
}

impl Keys {
    pub fn plain(id_key: [u8; 32]) -> Keys {
        Keys { seal: None, id_key }
    }

    pub fn from_master(master: &[u8; 32]) -> Keys {
        let data = blake3::derive_key("keepr 2026 data key", master);
        let id_key = blake3::derive_key("keepr 2026 id key", master);
        Keys { seal: Some(XChaCha20Poly1305::new((&data).into())), id_key }
    }

    pub fn encrypted(&self) -> bool {
        self.seal.is_some()
    }

    pub fn id(&self, plain: &[u8]) -> crate::Id {
        crate::Id(*blake3::keyed_hash(&self.id_key, plain).as_bytes())
    }

    /// Nonce then ciphertext and tag; unchanged when the repository isn't encrypted.
    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        match &self.seal {
            None => plain.to_vec(),
            Some(c) => seal_with(c, plain),
        }
    }

    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>> {
        match &self.seal {
            None => Ok(sealed.to_vec()),
            Some(c) => open_with(c, sealed),
        }
    }
}

fn seal_with(c: &XChaCha20Poly1305, plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; NONCE];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = c.encrypt(XNonce::from_slice(&nonce), plain).expect("encrypting into memory can't fail");
    let mut out = Vec::with_capacity(NONCE + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

fn open_with(c: &XChaCha20Poly1305, sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < NONCE + 16 {
        return Err(Error::new("damaged data: too short to decrypt"));
    }
    c.decrypt(XNonce::from_slice(&sealed[..NONCE]), &sealed[NONCE..])
        .map_err(|_| Error::new("damaged data, or the wrong key: it doesn't decrypt"))
}

/// How the master key is kept in `keepr-repo.json`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Encryption {
    pub kdf: Kdf,
    /// The master key sealed by the password's key, hex.
    pub password_key: String,
    /// The master key sealed by the recovery key's, hex.
    pub recovery_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Kdf {
    pub alg: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub salt: String,
}

impl Kdf {
    pub fn new() -> Kdf {
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        Kdf { alg: "argon2id".into(), m_kib: 64 * 1024, t: 3, p: 1, salt: hex::encode(salt) }
    }

    /// Cheap parameters, for tests only: the real ones take a noticeable fraction of a second.
    #[cfg(test)]
    pub fn fast() -> Kdf {
        Kdf { m_kib: 64, t: 1, ..Kdf::new() }
    }

    fn derive(&self, password: &str) -> Result<[u8; 32]> {
        if self.alg != "argon2id" {
            return Err(Error::new(format!("unknown key derivation {:?}", self.alg)));
        }
        let params = argon2::Params::new(self.m_kib, self.t, self.p, Some(32)).map_err(|e| Error::new(e.to_string()))?;
        let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let salt = hex::decode(&self.salt).map_err(|_| Error::new("damaged repository settings: bad salt"))?;
        let mut out = [0u8; 32];
        a.hash_password_into(password.as_bytes(), &salt, &mut out).map_err(|e| Error::new(e.to_string()))?;
        Ok(out)
    }
}

/// A recovery key: 32 random bytes as 52 characters of Crockford base32 in groups of 8, so it
/// can be read aloud, typed or printed.
pub fn new_recovery_key() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    let s = base32(&b);
    s.as_bytes().chunks(8).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("-")
}

const B32: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn base32(b: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &x in b {
        acc = (acc << 8) | x as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// The recovery key's own sealing key. It is 256 random bits, so no slow derivation is needed;
/// dashes, spaces and case are ignored so a hand-typed key still works.
fn recovery_kek(key: &str) -> [u8; 32] {
    let norm: String = key.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    blake3::derive_key("keepr 2026 recovery key", norm.as_bytes())
}

fn kek_cipher(kek: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(kek.into())
}

impl Encryption {
    /// A new master key, sealed by the password and a new recovery key (returned for the user to keep).
    pub fn create(password: &str, kdf: Kdf) -> Result<(Encryption, [u8; 32], String)> {
        let mut master = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut master);
        let recovery = new_recovery_key();
        let pk = kek_cipher(&kdf.derive(password)?);
        let rk = kek_cipher(&recovery_kek(&recovery));
        let e = Encryption { kdf, password_key: hex::encode(seal_with(&pk, &master)), recovery_key: hex::encode(seal_with(&rk, &master)) };
        Ok((e, master, recovery))
    }

    fn unwrap(c: &XChaCha20Poly1305, hexed: &str) -> Result<[u8; 32]> {
        let sealed = hex::decode(hexed).map_err(|_| Error::new("damaged repository settings"))?;
        let m = open_with(c, &sealed)?;
        m.try_into().map_err(|_| Error::new("damaged repository settings"))
    }

    pub fn open_with_password(&self, password: &str) -> Result<[u8; 32]> {
        Self::unwrap(&kek_cipher(&self.kdf.derive(password)?), &self.password_key).map_err(|_| Error::new("That password doesn't open this backup."))
    }

    pub fn open_with_recovery(&self, key: &str) -> Result<[u8; 32]> {
        Self::unwrap(&kek_cipher(&recovery_kek(key)), &self.recovery_key).map_err(|_| Error::new("That recovery key doesn't open this backup."))
    }

    /// Seals the same master key with a new password; the recovery key keeps working.
    pub fn with_new_password(&self, master: &[u8; 32], password: &str) -> Result<Encryption> {
        let kdf = Kdf { salt: Kdf::new().salt, ..self.kdf.clone() };
        let pk = kek_cipher(&kdf.derive(password)?);
        Ok(Encryption { kdf, password_key: hex::encode(seal_with(&pk, master)), recovery_key: self.recovery_key.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_and_recovery_open_the_same_key() {
        let (e, master, rec) = Encryption::create("hunter2", Kdf::fast()).unwrap();
        assert_eq!(e.open_with_password("hunter2").unwrap(), master);
        assert!(e.open_with_password("hunter3").is_err());
        assert_eq!(e.open_with_recovery(&rec).unwrap(), master);
        assert_eq!(e.open_with_recovery(&rec.to_lowercase().replace('-', " ")).unwrap(), master);
        let e2 = e.with_new_password(&master, "new").unwrap();
        assert_eq!(e2.open_with_password("new").unwrap(), master);
        assert!(e2.open_with_password("hunter2").is_err());
        assert_eq!(e2.open_with_recovery(&rec).unwrap(), master);
        assert_eq!(rec.len(), 52 + 6); // 256 bits of base32 is 52 characters, in groups of 8
    }

    #[test]
    fn sealing_round_trips_and_detects_damage() {
        let k = Keys::from_master(&[7; 32]);
        let s = k.seal(b"hello");
        assert_ne!(&s[NONCE..], b"hello");
        assert_eq!(k.open(&s).unwrap(), b"hello");
        let mut bad = s.clone();
        bad[NONCE] ^= 1;
        assert!(k.open(&bad).is_err());
        assert_ne!(k.id(b"x"), Keys::plain([0; 32]).id(b"x"));
    }
}
