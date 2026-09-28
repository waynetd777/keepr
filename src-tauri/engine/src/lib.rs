//! Keepr's backup repository.
//!
//! A repository is a folder (on this Mac, an external drive or a mounted SMB share) laid out as:
//!
//! | Path | What |
//! |---|---|
//! | `keepr-repo.json` | The format, the chunker's sizes and, when encrypted, the wrapped master key |
//! | `packs/ab/<id>.pack` | Blobs (file chunks and directory trees), compressed and sealed, ~16 MiB a pack |
//! | `index/<id>.idx` | Which pack holds each blob; one per backup, merged by prune |
//! | `snapshots/<id>.snap` | One per backup: when, which plan, the root tree |
//! | `locks/<id>.lock` | Who is using the repository |
//!
//! Files are split by content-defined chunking (FastCDC), so an edit in the middle of a big file
//! only stores the chunks around it, and a chunk already stored under any path in any snapshot is
//! never stored twice. Every snapshot is a complete picture of the sources; "incremental" only
//! means unchanged files weren't re-read.
//!
//! Write order is what keeps a crash harmless: packs, then the index naming them, then the
//! snapshot. A snapshot only exists once everything it points at does.

pub mod backend;
pub mod backup;
pub mod browse;
pub mod check;
pub mod crypto;
pub mod prune;
pub mod repo;
pub mod restore;
pub mod retention;
pub mod s3;
pub mod tree;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A blob's id: the keyed BLAKE3 hash of its plain contents. Keyed, so an encrypted repository's
/// ids say nothing about which well-known files it holds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Id(pub [u8; 32]);

impl Id {
    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
    pub fn parse(s: &str) -> Result<Id> {
        let v = hex::decode(s).map_err(|_| Error::new(format!("bad id {s:?}")))?;
        let a: [u8; 32] = v.try_into().map_err(|_| Error::new(format!("bad id {s:?}")))?;
        Ok(Id(a))
    }
    pub fn random() -> Id {
        use rand::RngCore;
        let mut a = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut a);
        Id(a)
    }
    /// The first 8 hex digits, for names people might see.
    pub fn short(&self) -> String {
        self.hex()[..8].to_string()
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.short())
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Id, D::Error> {
        let s = String::deserialize(d)?;
        Id::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Every failure is a sentence someone can read: the app shows them as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Error {
        Error(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error(format!("unreadable data: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Asked often during long work; a job sets it to stop, and the work returns `Error("cancelled")`.
pub fn cancelled() -> Error {
    Error::new("cancelled")
}
