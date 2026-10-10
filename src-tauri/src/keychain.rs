// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Passwords, in one login-Keychain item: service "Keepr", account "secrets", holding a JSON map of
//! `smb:<user>@<server>` (a share's password), `plan:<id>` (a backup's password) and
//! `recovery:<id>` (its recovery key, until the user has saved it elsewhere too).
//!
//! One item, read once per run and kept in memory, because every new build of a self-signed app
//! is a stranger to the Keychain: its partition list names the build's own code hash (there is no
//! Team ID to name instead), so macOS asks again for each item after every rebuild whatever
//! "Always Allow" said before. One item means one question per build rather than one per secret.
//! Items from before this (one per secret) are moved in the first time each is asked for.

use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
use std::collections::BTreeMap;
use std::sync::Mutex;

const SERVICE: &str = "Keepr";
const ACCOUNT: &str = "secrets";

static CACHE: Mutex<Option<BTreeMap<String, String>>> = Mutex::new(None);

/// What the Keychain said when asked for the item: its bytes, or the OSStatus it failed with.
type Read = Result<Vec<u8>, i32>;

/// The Keychain's "no such item" status. Only this one means there are no secrets yet.
const ITEM_NOT_FOUND: i32 = -25300;

/// Turns a read of the item into the map it holds. Only "not found" counts as empty: any other
/// failure (the user pressed Deny, the Keychain is locked, there's nobody to answer the prompt, or
/// the item doesn't parse) is an error, because treating it as empty and then saving one new secret
/// would write over the item and lose every backup password and recovery key in it.
fn parse(read: Read) -> Result<BTreeMap<String, String>, String> {
    match read {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("Keepr's Keychain item couldn't be read ({e}), so nothing was saved to it")),
        Err(ITEM_NOT_FOUND) => Ok(BTreeMap::new()),
        Err(code) => Err(format!("The Keychain couldn't be read (error {code}), so nothing was saved to it. Unlock it, or allow Keepr when macOS asks, and try again")),
    }
}

/// The secrets, read from the Keychain the first time and kept. A failed read is not kept, so the
/// next call asks again rather than carrying on with nothing.
fn load(cache: &mut Option<BTreeMap<String, String>>) -> Result<&mut BTreeMap<String, String>, String> {
    load_with(cache, || get_generic_password(SERVICE, ACCOUNT).map_err(|e| e.code()))
}

fn load_with(cache: &mut Option<BTreeMap<String, String>>, read: impl FnOnce() -> Read) -> Result<&mut BTreeMap<String, String>, String> {
    if cache.is_none() {
        *cache = Some(parse(read())?);
    }
    Ok(cache.as_mut().expect("just filled"))
}

fn store(map: &BTreeMap<String, String>) -> Result<(), String> {
    let json = serde_json::to_vec(map).map_err(|e| e.to_string())?;
    set_generic_password(SERVICE, ACCOUNT, &json).map_err(|e| format!("Couldn't save to the Keychain: {e}"))
}

/// Reads Keepr's Keychain item now, so that if macOS needs to ask (a new build, or "Deny"
/// last time) it asks at startup, while someone is looking, not in the middle of the night
/// when a backup needs a password and nobody is there to answer.
pub fn warm() {
    let mut c = CACHE.lock().unwrap();
    if let Err(e) = load(&mut c) {
        eprintln!("keychain: {e}");
    }
}

pub fn get(account: &str) -> Option<String> {
    let mut c = CACHE.lock().unwrap();
    let old = || get_generic_password(SERVICE, account).ok().and_then(|b| String::from_utf8(b).ok());
    let Ok(map) = load(&mut c) else {
        // The item couldn't be read, so an old one-per-secret item can be used but not moved in:
        // moving it would mean saving the item, which would write over what couldn't be read.
        return old();
    };
    if let Some(v) = map.get(account) {
        return Some(v.clone());
    }
    // An item from before everything was kept in one: move it in.
    let old = old()?;
    map.insert(account.to_string(), old.clone());
    if store(map).is_ok() {
        let _ = delete_generic_password(SERVICE, account);
    }
    Some(old)
}

pub fn set(account: &str, secret: &str) -> Result<(), String> {
    let mut c = CACHE.lock().unwrap();
    let map = load(&mut c)?;
    if map.get(account).map(String::as_str) == Some(secret) {
        return Ok(());
    }
    map.insert(account.to_string(), secret.to_string());
    store(map)
}

pub fn delete(account: &str) {
    let mut c = CACHE.lock().unwrap();
    match load(&mut c) {
        Ok(map) => {
            if map.remove(account).is_some() {
                let _ = store(map);
            }
        }
        // Leaving a stale secret behind is better than saving over the ones that couldn't be read.
        Err(e) => eprintln!("keychain: {e}"),
    }
    let _ = delete_generic_password(SERVICE, account);
}

/// A login for an SMB server that's already saved: Keepr's own, else one Finder saved ("Remember
/// this password" in Connect to Server). Returns the user name and where it came from; the
/// password itself is only read when it's used.
pub fn saved_smb_user(server: &str) -> Option<(String, &'static str)> {
    let host = server.trim().to_lowercase();
    if host.is_empty() {
        return None;
    }
    {
        let mut c = CACHE.lock().unwrap();
        if let Some(user) = load(&mut c).ok().and_then(|map| {
            map.keys().find_map(|k| k.strip_prefix("smb:").and_then(|r| r.strip_suffix(&format!("@{host}"))).map(str::to_string))
        }) {
            return Some((user, "keepr"));
        }
    }
    // Finder's item: its attributes (not its password) can be read without asking.
    for h in [server.trim().to_string(), server.trim().trim_end_matches(".local").to_string()] {
        let out = std::process::Command::new("/usr/bin/security").args(["find-internet-password", "-s", &h, "-r", "smb "]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(acct) = text.lines().find_map(|l| l.trim().strip_prefix("\"acct\"<blob>=\"").and_then(|r| r.strip_suffix('"'))) {
            return Some((acct.to_string(), "finder"));
        }
    }
    None
}

/// The password Finder saved for an SMB server, if there is one. macOS may ask once to let Keepr
/// read Finder's item.
pub fn finder_smb_password(server: &str, user: &str) -> Option<String> {
    use security_framework::os::macos::passwords::find_internet_password;
    use security_framework_sys::keychain::{SecAuthenticationType, SecProtocolType};
    for h in [server.trim(), server.trim().trim_end_matches(".local")] {
        if let Ok((pw, _)) = find_internet_password(None, h, None, user, "", None, SecProtocolType::SMB, SecAuthenticationType::Any) {
            if let Ok(s) = String::from_utf8(pw.to_owned()) {
                return Some(s);
            }
        }
    }
    None
}

pub fn smb_account(user: &str, server: &str) -> String {
    format!("smb:{}@{}", user, server.to_lowercase())
}

pub fn s3_account(access_key: &str) -> String {
    format!("s3:{}", access_key.trim())
}

pub fn plan_account(id: &str) -> String {
    format!("plan:{id}")
}

pub fn recovery_account(id: &str) -> String {
    format!("recovery:{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_item_reads_as_empty() {
        assert!(parse(Err(ITEM_NOT_FOUND)).unwrap().is_empty());
        // Deny, locked, and no one there to answer: none of these may become an empty map.
        for code in [-128, -25293, -25308, -25291] {
            assert!(parse(Err(code)).is_err(), "{code}");
        }
        assert!(parse(Ok(b"not json".to_vec())).is_err());
        assert_eq!(parse(Ok(br#"{"plan:a":"pw"}"#.to_vec())).unwrap()["plan:a"], "pw");
    }

    #[test]
    fn a_failed_read_is_not_cached() {
        let mut cache = None;
        assert!(load_with(&mut cache, || Err(-128)).is_err());
        assert!(cache.is_none());
        // The next call asks again, and a good read is then kept.
        assert!(load_with(&mut cache, || Ok(br#"{"plan:a":"pw"}"#.to_vec())).is_ok());
        assert_eq!(load_with(&mut cache, || Err(-128)).unwrap()["plan:a"], "pw");
    }
}
