// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Connecting to SMB shares on Windows, as smb.rs does on macOS: a connection to \\server\share
//! with no drive letter (WNetAddConnection2), which File Explorer's own connections and mapped
//! drives sit beside. Windows reaches a share by its UNC path once it's connected, so that path is
//! where the share "is mounted".
//!
//! A share someone already has connected (a mapped drive, a folder opened in File Explorer) is
//! used as it is and left connected. One Keepr connects itself is recorded, so it can be
//! disconnected when no backup or restore has needed it for a while. A connection made with a name
//! and no password lets Windows use the password it saved for that server, which no app can read.

use std::path::PathBuf;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, HANDLE, NO_ERROR, WIN32_ERROR};
use windows::Win32::NetworkManagement::WNet::{
    WNetAddConnection2W, WNetCancelConnection2W, WNetCloseEnum, WNetEnumResourceW, WNetOpenEnumW, NETRESOURCEW, NET_CONNECT_FLAGS,
    RESOURCETYPE_DISK, RESOURCE_CONNECTED, WNET_OPEN_ENUM_USAGE,
};

fn norm_host(h: &str) -> String {
    h.trim().trim_start_matches('\\').trim_end_matches('.').trim_end_matches(".local").to_lowercase()
}

/// \\server\share, as Windows names a share.
fn unc(server: &str, share: &str) -> String {
    format!(r"\\{}\{}", server.trim().trim_start_matches('\\'), share.trim_matches(['/', '\\']))
}

/// "\\host\share" split into host and share.
fn split_unc(remote: &str) -> Option<(String, String)> {
    let rest = remote.strip_prefix(r"\\")?;
    let (host, share) = rest.split_once('\\')?;
    Some((host.to_string(), share.trim_end_matches('\\').to_string()))
}

/// The disk shares connected now, as \\server\share: deviceless connections and mapped drives.
fn connections() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut h = HANDLE::default();
        if WNetOpenEnumW(RESOURCE_CONNECTED, RESOURCETYPE_DISK, WNET_OPEN_ENUM_USAGE(0), None, &mut h) != NO_ERROR {
            return out;
        }
        let mut buf = vec![0u64; 16 * 1024 / 8];
        loop {
            let mut count = u32::MAX;
            let mut size = (buf.len() * 8) as u32;
            let r = WNetEnumResourceW(h, &mut count, buf.as_mut_ptr().cast(), &mut size);
            if r == ERROR_MORE_DATA {
                buf = vec![0u64; size as usize / 8 + 1];
                continue;
            }
            if r != NO_ERROR {
                break;
            }
            let items = std::slice::from_raw_parts(buf.as_ptr() as *const NETRESOURCEW, count as usize);
            for it in items {
                if let Ok(remote) = it.lpRemoteName.to_string() {
                    out.push(remote);
                }
            }
            if r == ERROR_NO_MORE_ITEMS {
                break;
            }
        }
        let _ = WNetCloseEnum(h);
    }
    out
}

/// Servers of SMB shares connected now ("192.168.1.20", "keep-nas").
pub fn mounted_servers() -> Vec<String> {
    connections().iter().filter_map(|c| split_unc(c)).map(|(h, _)| h).collect()
}

/// Where `server`'s `share` is reached when it's connected already: its UNC path.
pub fn find_mount(server: &str, share: &str) -> Option<PathBuf> {
    let (want_host, want_share) = (norm_host(server), share.trim_matches(['/', '\\']).to_lowercase());
    connections()
        .iter()
        .filter_map(|c| split_unc(c))
        .any(|(h, s)| norm_host(&h) == want_host && s.to_lowercase() == want_share)
        .then(|| PathBuf::from(unc(server, share)))
}

/// Why Windows refused a connection, in words.
fn refusal(server: &str, share: &str, e: WIN32_ERROR) -> String {
    match e.0 {
        5 | 86 | 1326 | 1327 | 1330 | 1331 | 2202 => format!("{server} didn't accept the name and password."),
        67 => format!("{server} has no share called {share}."),
        53 | 1203 | 1231 | 1232 | 1222 => format!("{server} can't be reached. Is it on and on this network?"),
        1219 => format!("Windows is already connected to {server} as someone else. Disconnect it in File Explorer, or use that name."),
        n => format!("Couldn't connect to {server}/{share} (error {n})."),
    }
}

fn connect(remote: &str, user: &str, password: &str) -> WIN32_ERROR {
    let mut remote_w: Vec<u16> = remote.encode_utf16().chain([0]).collect();
    let res = NETRESOURCEW { dwType: RESOURCETYPE_DISK, lpRemoteName: PWSTR(remote_w.as_mut_ptr()), ..Default::default() };
    let (pw, name) = (HSTRING::from(password), HSTRING::from(user));
    unsafe {
        WNetAddConnection2W(
            &res,
            if password.is_empty() { PCWSTR::null() } else { PCWSTR(pw.as_ptr()) },
            if user.is_empty() { PCWSTR::null() } else { PCWSTR(name.as_ptr()) },
            NET_CONNECT_FLAGS(0),
        )
    }
}

/// Connects the share, returning where it's reached. The password goes to Windows directly, never
/// on a command line; with none, Windows uses the one it saved for the server. No dialog is shown:
/// an unattended backup can't answer one.
pub fn mount(server: &str, share: &str, user: &str, password: &str) -> Result<PathBuf, String> {
    let remote = unc(server, share);
    match connect(&remote, user, password) {
        NO_ERROR => Ok(PathBuf::from(remote)),
        // Connected meanwhile, or already (with the same name, or none given).
        e if find_mount(server, share).is_some() && e.0 != 1326 => Ok(PathBuf::from(remote)),
        e => Err(refusal(server, share, e)),
    }
}

/// Disconnects a share Keepr connected; false if it's still connected (a file open on it, say).
pub fn unmount(path: &std::path::Path) -> bool {
    let name = HSTRING::from(path.as_os_str());
    unsafe { WNetCancelConnection2W(&name, NET_CONNECT_FLAGS(0), false) == NO_ERROR }
}

/// Servers offering SMB on this network, by their Bonjour names ("keep-nas"), through Windows' own
/// DNS-SD browsing (Windows 10 1809 and later). Takes about a second. Windows PCs themselves don't
/// announce their shares this way; a NAS usually does.
pub fn discover() -> Vec<String> {
    use std::ffi::c_void;
    use std::sync::Mutex;
    use windows::Win32::NetworkManagement::Dns::{
        DnsFree, DnsFreeRecordList, DnsServiceBrowse, DnsServiceBrowseCancel, DNS_QUERY_REQUEST_VERSION1, DNS_RECORDW,
        DNS_SERVICE_BROWSE_REQUEST, DNS_SERVICE_BROWSE_REQUEST_0, DNS_SERVICE_CANCEL, DNS_TYPE_PTR,
    };
    unsafe extern "system" fn found(_status: u32, ctx: *const c_void, rec: *const DNS_RECORDW) {
        if ctx.is_null() {
            return;
        }
        let names = &*(ctx as *const Mutex<Vec<String>>);
        let mut r = rec;
        while !r.is_null() {
            if (*r).wType == DNS_TYPE_PTR.0 {
                if let Ok(host) = (*r).Data.PTR.pNameHost.to_string() {
                    let name = host.split("._smb._tcp").next().unwrap_or(&host).to_string();
                    if !name.is_empty() {
                        names.lock().unwrap().push(name);
                    }
                }
            }
            r = (*r).pNext;
        }
        if !rec.is_null() {
            DnsFree(Some(rec as *const c_void), DnsFreeRecordList);
        }
    }
    // Kept alive past the cancel: a reply already on its way may still arrive.
    let names: &'static Mutex<Vec<String>> = Box::leak(Box::new(Mutex::new(Vec::new())));
    let query = HSTRING::from("_smb._tcp.local");
    let req = DNS_SERVICE_BROWSE_REQUEST {
        Version: DNS_QUERY_REQUEST_VERSION1.0,
        InterfaceIndex: 0,
        QueryName: PCWSTR(query.as_ptr()),
        Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 { pBrowseCallback: Some(found) },
        pQueryContext: names as *const Mutex<Vec<String>> as *mut c_void,
    };
    let mut cancel = DNS_SERVICE_CANCEL::default();
    unsafe {
        // DNS_REQUEST_PENDING
        if DnsServiceBrowse(&req, &mut cancel) != 9506 {
            return vec![];
        }
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let _ = DnsServiceBrowseCancel(&cancel);
    }
    let mut out = names.lock().unwrap().clone();
    out.sort_by_key(|n| n.to_lowercase());
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    out
}

/// The shares a server offers (not the hidden ones ending "$"), asking with the name and password:
/// a connection to its IPC$ signs in, then the server is asked for its list (NetShareEnum).
pub fn shares(server: &str, user: &str, password: &str) -> Result<Vec<String>, String> {
    use windows::Win32::NetworkManagement::NetManagement::{NetApiBufferFree, MAX_PREFERRED_LENGTH};
    use windows::Win32::Storage::FileSystem::{NetShareEnum, SHARE_INFO_1, STYPE_DISKTREE, STYPE_MASK};
    let ipc = unc(server, "IPC$");
    // Signing in with the name given; with no name, as Windows would (its saved login, or as guest).
    let signed_in = !user.is_empty() && connect(&ipc, user, password) == NO_ERROR;
    let host = HSTRING::from(format!(r"\\{}", server.trim().trim_start_matches('\\')));
    let mut names = Vec::new();
    let status = unsafe {
        let mut buf: *mut u8 = std::ptr::null_mut();
        let (mut read, mut total) = (0u32, 0u32);
        let r = NetShareEnum(&host, 1, &mut buf, MAX_PREFERRED_LENGTH, &mut read, &mut total, None);
        if r == 0 && !buf.is_null() {
            for s in std::slice::from_raw_parts(buf as *const SHARE_INFO_1, read as usize) {
                let name = s.shi1_netname.to_string().unwrap_or_default();
                if s.shi1_type.0 & STYPE_MASK.0 == STYPE_DISKTREE.0 && !name.is_empty() && !name.ends_with('$') {
                    names.push(name);
                }
            }
        }
        if !buf.is_null() {
            NetApiBufferFree(Some(buf as *const std::ffi::c_void));
        }
        r
    };
    if signed_in {
        unsafe {
            let _ = WNetCancelConnection2W(&HSTRING::from(ipc), NET_CONNECT_FLAGS(0), false);
        }
    }
    if names.is_empty() {
        return Err(match status {
            5 | 86 | 1326 => format!("{server} didn't accept the name and password."),
            53 | 1231 | 1232 => format!("{server} can't be reached. Is it on and on this network?"),
            _ => format!("{server} didn't list any shares."),
        });
    }
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_shares() {
        assert_eq!(unc("keep-nas", "/Backups/"), r"\\keep-nas\Backups");
        assert_eq!(split_unc(r"\\keep-nas\My Share"), Some(("keep-nas".into(), "My Share".into())));
        assert_eq!(norm_host("Keep-NAS.local."), "keep-nas");
        assert!(find_mount("keepr-no-such-server", "x").is_none());
    }

    /// Against a real server: KEEPR_TEST_SMB_HOST=192.168.1.20 cargo test smb -- --ignored
    #[test]
    #[ignore]
    fn lists_a_real_servers_shares_as_guest() {
        let host = std::env::var("KEEPR_TEST_SMB_HOST").expect("KEEPR_TEST_SMB_HOST");
        let s = shares(&host, "", "").unwrap();
        assert!(!s.is_empty());
        println!("shares: {s:?}; servers: {:?}", discover());
    }
}
