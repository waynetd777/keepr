//! Passwords in the login Keychain, under the service "Keepr".
//!
//! Items Keepr creates are readable by Keepr without a prompt as long as the app keeps the same
//! signature, which is why it is signed with a stable certificate (signing.local). An unattended
//! backup that stopped to ask for Keychain access would wait for someone who isn't there.
//!
//! Accounts: `smb:<user>@<server>` (a share's password), `plan:<id>` (a backup's password) and
//! `recovery:<id>` (its recovery key, until the user has saved it somewhere else too).

use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

const SERVICE: &str = "Keepr";

pub fn get(account: &str) -> Option<String> {
    get_generic_password(SERVICE, account).ok().and_then(|b| String::from_utf8(b).ok())
}

pub fn set(account: &str, secret: &str) -> Result<(), String> {
    set_generic_password(SERVICE, account, secret.as_bytes()).map_err(|e| format!("Couldn't save to the Keychain: {e}"))
}

pub fn delete(account: &str) {
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
