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

pub fn smb_account(user: &str, server: &str) -> String {
    format!("smb:{}@{}", user, server.to_lowercase())
}

pub fn plan_account(id: &str) -> String {
    format!("plan:{id}")
}

pub fn recovery_account(id: &str) -> String {
    format!("recovery:{id}")
}
