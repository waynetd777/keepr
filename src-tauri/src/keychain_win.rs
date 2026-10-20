// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Passwords on Windows, as keychain.rs keeps them on macOS: in Windows' Credential Manager, one
//! generic credential per secret, named `Keepr/<account>` (`Keepr/smb:<user>@<server>`,
//! `Keepr/plan:<id>`, `Keepr/recovery:<id>`, `Keepr/s3:<key>`), readable only by the signed-in
//! user. macOS's reason for one item holding them all (a question per item after every rebuild)
//! doesn't arise: Windows never asks. They show in Control Panel › Credential Manager › Windows
//! Credentials.

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredEnumerateW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_ENUMERATE_FLAGS, CRED_FLAGS,
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_DOMAIN_PASSWORD, CRED_TYPE_GENERIC,
};

const PREFIX: &str = "Keepr/";

/// Credential Manager's limit on a credential's secret, in bytes.
const MAX_BLOB: usize = 5 * 512;

/// Nothing to warm: Windows doesn't ask the user before an app reads its own credentials.
pub fn warm() {}

fn target(account: &str) -> HSTRING {
    HSTRING::from(format!("{PREFIX}{account}"))
}

pub fn get(account: &str) -> Option<String> {
    unsafe {
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        CredReadW(&target(account), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
        let c = &*cred;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
        CredFree(cred as *const _);
        String::from_utf8(bytes).ok()
    }
}

pub fn set(account: &str, secret: &str) -> Result<(), String> {
    if get(account).as_deref() == Some(secret) {
        return Ok(());
    }
    let mut blob = secret.as_bytes().to_vec();
    if blob.len() > MAX_BLOB {
        return Err("That's too long for Windows' Credential Manager.".into());
    }
    let mut name: Vec<u16> = format!("{PREFIX}{account}").encode_utf16().chain([0]).collect();
    let mut user: Vec<u16> = "Keepr".encode_utf16().chain([0]).collect();
    let cred = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        Comment: PWSTR::null(),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR(user.as_mut_ptr()),
    };
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| format!("Couldn't save to Windows' Credential Manager: {}", e.message()))
}

pub fn delete(account: &str) {
    unsafe {
        let _ = CredDeleteW(&target(account), CRED_TYPE_GENERIC, None);
    }
}

/// The accounts Keepr has saved whose names start with `prefix` ("smb:").
fn accounts(prefix: &str) -> Vec<String> {
    let filter = HSTRING::from(format!("{PREFIX}{prefix}*"));
    let mut out = Vec::new();
    unsafe {
        let mut n = 0u32;
        let mut list: *mut *mut CREDENTIALW = std::ptr::null_mut();
        if CredEnumerateW(&filter, Some(CRED_ENUMERATE_FLAGS(0)), &mut n, &mut list).is_err() {
            return out;
        }
        for i in 0..n as usize {
            let c = &**list.add(i);
            if let Ok(t) = c.TargetName.to_string() {
                if let Some(a) = t.strip_prefix(PREFIX) {
                    out.push(a.to_string());
                }
            }
        }
        CredFree(list as *const _);
    }
    out
}

/// A login for an SMB server that's already saved: Keepr's own, else the one Windows keeps for
/// that server ("Remember my credentials" when connecting in File Explorer). Returns the user name
/// and where it came from; Windows never lets an app read its password, but connecting with that
/// name and no password lets Windows use it (smb_win.rs).
pub fn saved_smb_user(server: &str) -> Option<(String, &'static str)> {
    let host = server.trim().to_lowercase();
    if host.is_empty() {
        return None;
    }
    if let Some(user) =
        accounts("smb:").iter().find_map(|k| k.strip_prefix("smb:").and_then(|r| r.strip_suffix(&format!("@{host}"))).map(str::to_string))
    {
        return Some((user, "keepr"));
    }
    for h in [server.trim().to_string(), server.trim().trim_end_matches(".local").to_string()] {
        unsafe {
            let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
            if CredReadW(&HSTRING::from(h), CRED_TYPE_DOMAIN_PASSWORD, None, &mut cred).is_ok() {
                let user = (*cred).UserName.to_string().ok();
                CredFree(cred as *const _);
                if let Some(u) = user.filter(|u| !u.is_empty()) {
                    // "SERVER\name": the name is what a connection asks for.
                    let u = u.rsplit('\\').next().unwrap_or(&u).to_string();
                    return Some((u, "windows"));
                }
            }
        }
    }
    None
}

/// macOS's Finder keeps share passwords Keepr may read; Windows keeps them where no app can, so
/// there is none to give. A connection without one uses Windows' own (smb_win::mount).
pub fn finder_smb_password(_server: &str, _user: &str) -> Option<String> {
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
    fn saves_reads_and_deletes_a_secret() {
        let account = format!("test:{}", std::process::id());
        delete(&account);
        assert_eq!(get(&account), None);
        // A session with no logon of its own (SSH, a service) has no Credential Manager to write to.
        if let Err(e) = set(&account, "pässword") {
            assert!(e.contains("logon session"), "{e}");
            println!("no Credential Manager in this session: {e}");
            return;
        }
        assert_eq!(get(&account).as_deref(), Some("pässword"));
        assert!(accounts("test:").contains(&account));
        set(&account, "another").unwrap();
        assert_eq!(get(&account).as_deref(), Some("another"));
        delete(&account);
        assert_eq!(get(&account), None);
        assert!(set(&account, &"x".repeat(MAX_BLOB + 1)).is_err());
    }
}
