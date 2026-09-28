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

fn load(cache: &mut Option<BTreeMap<String, String>>) -> &mut BTreeMap<String, String> {
    cache.get_or_insert_with(|| get_generic_password(SERVICE, ACCOUNT).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default())
}

fn store(map: &BTreeMap<String, String>) -> Result<(), String> {
    let json = serde_json::to_vec(map).map_err(|e| e.to_string())?;
    set_generic_password(SERVICE, ACCOUNT, &json).map_err(|e| format!("Couldn't save to the Keychain: {e}"))
}

pub fn get(account: &str) -> Option<String> {
    let mut c = CACHE.lock().unwrap();
    let map = load(&mut c);
    if let Some(v) = map.get(account) {
        return Some(v.clone());
    }
    // An item from before everything was kept in one: move it in.
    let old = get_generic_password(SERVICE, account).ok().and_then(|b| String::from_utf8(b).ok())?;
    map.insert(account.to_string(), old.clone());
    if store(map).is_ok() {
        let _ = delete_generic_password(SERVICE, account);
    }
    Some(old)
}

pub fn set(account: &str, secret: &str) -> Result<(), String> {
    let mut c = CACHE.lock().unwrap();
    let map = load(&mut c);
    if map.get(account).map(String::as_str) == Some(secret) {
        return Ok(());
    }
    map.insert(account.to_string(), secret.to_string());
    store(map)
}

pub fn delete(account: &str) {
    let mut c = CACHE.lock().unwrap();
    let map = load(&mut c);
    if map.remove(account).is_some() {
        let _ = store(map);
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
        let map = load(&mut c);
        if let Some(user) = map.keys().find_map(|k| k.strip_prefix("smb:").and_then(|r| r.strip_suffix(&format!("@{host}"))).map(str::to_string)) {
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
