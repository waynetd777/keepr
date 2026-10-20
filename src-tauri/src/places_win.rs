// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What places.rs asks of Windows: drives and their names, free space, and the cloud services'
//! sync folders.
//!
//! Paths here are the app's (keepr_engine::paths): "/Users/w/OneDrive" for C:\Users\w\OneDrive,
//! "D:/Backups" for a folder on another drive.

use keepr_engine::paths;
use std::path::Path;
use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW};

/// The drive an app path is on, when it isn't the system drive: "D:".
pub fn drive_of(path: &str) -> Option<String> {
    let b = path.as_bytes();
    (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()).then(|| path[..2].to_uppercase())
}

/// A drive's root as Windows names it ("D:\"), for an app path's drive letter.
fn root(drive: &str) -> HSTRING {
    HSTRING::from(format!("{drive}\\"))
}

/// Whether a drive is there now: plugged in, or a mapped network drive that's reachable.
pub fn drive_connected(drive: &str) -> bool {
    // DRIVE_NO_ROOT_DIR (1): nothing at that letter.
    let kind = unsafe { GetDriveTypeW(&root(drive)) };
    kind > 1 && Path::new(&format!("{drive}\\")).exists()
}

/// A drive's label as File Explorer shows it ("Archive SSD"), None without one or when it's not there.
pub fn label(drive: &str) -> Option<String> {
    let mut name = [0u16; 261];
    unsafe { GetVolumeInformationW(&root(drive), Some(&mut name), None, None, None, None) }.ok()?;
    let n = name.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&name[..n]).trim().to_string()).filter(|s| !s.is_empty())
}

/// A drive by name: its label, else the letter ("D:").
pub fn drive_name(drive: &str) -> String {
    label(drive).unwrap_or_else(|| drive.to_string())
}

/// Free and total bytes on the volume holding `p`.
pub fn space(p: &Path) -> Option<(u64, u64)> {
    let (mut free, mut total) = (0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(p.as_os_str()), Some(&mut free), Some(&mut total), None) }.ok()?;
    Some((free, total))
}

/// A cloud service's sync folder on this PC.
#[derive(Clone, Debug)]
pub struct SyncRoot {
    /// "onedrive", "icloud", "google", "dropbox" or "other".
    pub provider: &'static str,
    /// "OneDrive Personal", "iCloud Drive".
    pub name: String,
    /// The folder, in the app's form.
    pub root: String,
}

fn provider_of(id: &str) -> &'static str {
    let r = id.to_lowercase();
    if r.starts_with("onedrive") {
        "onedrive"
    } else if r.contains("google") {
        "google"
    } else if r.starts_with("dropbox") {
        "dropbox"
    } else if r.contains("icloud") {
        "icloud"
    } else {
        "other"
    }
}

/// "OneDrive - Personal" → "OneDrive Personal"; "OneDrive - Contoso" → "OneDrive Contoso".
fn name_of(provider: &str, folder: &str) -> String {
    let leaf = folder.rsplit('/').next().unwrap_or(folder);
    let leaf = leaf.replace(" - ", " ");
    match provider {
        "icloud" => "iCloud Drive".into(),
        "google" => "Google Drive".into(),
        "dropbox" if leaf.eq_ignore_ascii_case("dropbox") => "Dropbox".into(),
        "onedrive" if leaf.eq_ignore_ascii_case("onedrive") => "OneDrive".into(),
        _ => leaf,
    }
}

/// Every sync folder Windows knows for this user: what the cloud services registered with Windows'
/// cloud files (HKLM\…\Explorer\SyncRootManager, each with its folder per user), which OneDrive,
/// iCloud for Windows, Dropbox and Box use; and OneDrive's own folders from its environment
/// variables, there whenever it's signed in. Google Drive's drive letter (G:, "Google Drive") is
/// found by its label. A root that's registered is live: signing out takes it away.
pub fn sync_roots() -> Vec<SyncRoot> {
    use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ};
    let mut out: Vec<SyncRoot> = Vec::new();
    let mut add = |provider: &'static str, folder: &Path| {
        if !folder.is_dir() {
            return;
        }
        let root = paths::app(folder);
        if out.iter().any(|r| r.root.eq_ignore_ascii_case(&root)) {
            return;
        }
        out.push(SyncRoot { provider, name: name_of(provider, &root), root });
    };
    let base = HSTRING::from(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager");
    let sid = user_sid();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, &base, None, KEY_READ, &mut key).is_ok() {
            for i in 0.. {
                let mut name = [0u16; 512];
                let mut len = name.len() as u32;
                if RegEnumKeyExW(key, i, Some(windows::core::PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                    break;
                }
                let id = String::from_utf16_lossy(&name[..len as usize]);
                // "OneDrive!S-1-5-21-…!Personal|…": one per user; only this user's.
                if sid.as_ref().is_some_and(|s| id.contains('!') && !id.contains(s.as_str())) {
                    continue;
                }
                let mut sub = HKEY::default();
                let path = HSTRING::from(format!(r"{base}\{id}\UserSyncRoots"));
                if RegOpenKeyExW(HKEY_LOCAL_MACHINE, &path, None, KEY_READ, &mut sub).is_err() {
                    continue;
                }
                for j in 0.. {
                    let mut vname = [0u16; 256];
                    let mut vlen = vname.len() as u32;
                    let mut data = [0u16; 1024];
                    let mut dlen = (data.len() * 2) as u32;
                    if RegEnumValueW(
                        sub,
                        j,
                        Some(windows::core::PWSTR(vname.as_mut_ptr())),
                        &mut vlen,
                        None,
                        None,
                        Some(data.as_mut_ptr().cast()),
                        Some(&mut dlen),
                    )
                    .is_err()
                    {
                        break;
                    }
                    let value_sid = String::from_utf16_lossy(&vname[..vlen as usize]);
                    if sid.as_ref().is_some_and(|s| !value_sid.eq_ignore_ascii_case(s)) {
                        continue;
                    }
                    let n = (dlen as usize / 2).min(data.len());
                    let folder = String::from_utf16_lossy(&data[..n]).trim_end_matches('\0').to_string();
                    add(provider_of(&id), Path::new(&folder));
                }
                let _ = RegCloseKey(sub);
            }
            let _ = RegCloseKey(key);
        }
    }
    for var in ["OneDriveConsumer", "OneDriveCommercial", "OneDrive"] {
        if let Some(d) = std::env::var_os(var) {
            add("onedrive", Path::new(&d));
        }
    }
    for d in 'D'..='Z' {
        let drive = format!("{d}:");
        if label(&drive).is_some_and(|l| l.eq_ignore_ascii_case("Google Drive")) {
            let top = std::path::PathBuf::from(format!("{drive}\\"));
            let my = top.join("My Drive");
            add("google", if my.is_dir() { &my } else { &top });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The sync roots, looked up at most once a minute: they're asked about for every place shown.
pub fn sync_roots_cached() -> Vec<SyncRoot> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<SyncRoot>)>> = std::sync::Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if let Some((at, roots)) = c.as_ref() {
        if at.elapsed() < std::time::Duration::from_secs(60) {
            return roots.clone();
        }
    }
    let roots = sync_roots();
    *c = Some((std::time::Instant::now(), roots.clone()));
    roots
}

/// The sync root an app path is in, if it is in one.
pub fn sync_root_of(path: &str) -> Option<SyncRoot> {
    let low = path.to_lowercase();
    sync_roots_cached().into_iter().find(|r| {
        let root = r.root.to_lowercase();
        low == root || low.starts_with(&format!("{}/", root.trim_end_matches('/')))
    })
}

/// The signed-in user's SID ("S-1-5-21-…"), which names their entries in the registry.
pub fn user_sid() -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut buf = vec![0u8; 256];
        let mut len = 0u32;
        let ok = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), buf.len() as u32, &mut len).is_ok();
        let _ = CloseHandle(token);
        if !ok {
            return None;
        }
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s = windows::core::PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut s).ok()?;
        let sid = s.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(s.0.cast())));
        sid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives_and_sync_roots() {
        assert_eq!(drive_of("D:/Backups").as_deref(), Some("D:"));
        assert_eq!(drive_of("/Users/w"), None);
        assert!(drive_connected(paths::system_drive()));
        assert!(!drive_connected("Q:") || Path::new("Q:\\").exists());
        assert_eq!(name_of("onedrive", "/Users/w/OneDrive - Personal"), "OneDrive Personal");
        assert_eq!(name_of("onedrive", "/Users/w/OneDrive"), "OneDrive");
        assert_eq!(name_of("icloud", "/Users/w/iCloudDrive"), "iCloud Drive");
        assert!(user_sid().is_some_and(|s| s.starts_with("S-1-")));
        println!("sync roots: {:?}", sync_roots());
    }
}
