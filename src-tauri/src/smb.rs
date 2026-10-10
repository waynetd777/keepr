// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Connecting to SMB shares, through NetFS: the framework Finder's Connect to Server uses.
//!
//! A share someone already has mounted is used where it is and left mounted. One Keepr mounts
//! itself (hidden from the Desktop and Finder's sidebar) is recorded, so it can be unmounted when
//! no backup or restore has needed it for a while.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFMutableDictionary, CFMutableDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURLRef, CFURL};
use std::ffi::CStr;
use std::path::PathBuf;

#[link(name = "NetFS", kind = "framework")]
extern "C" {
    fn NetFSMountURLSync(
        url: CFURLRef,
        mountpath: CFURLRef,
        user: CFStringRef,
        passwd: CFStringRef,
        open_options: CFMutableDictionaryRef,
        mount_options: CFMutableDictionaryRef,
        mountpoints: *mut CFArrayRef,
    ) -> i32;
}

/// Hide the mount from the Desktop and Finder's sidebar (sys/mount.h).
const MNT_DONTBROWSE: i64 = 0x0010_0000;

fn norm_host(h: &str) -> String {
    h.trim().trim_end_matches('.').trim_end_matches(".local").to_lowercase()
}

/// Percent-decodes what getmntinfo reports ("//wayne@keep-nas._smb._tcp.local/My%20Share").
/// Works on bytes throughout: slicing the string after a "%" would panic when what follows is a
/// character of more than one byte ("%é").
fn decode(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Every mounted volume, in a buffer of our own. (getmntinfo hands out one static buffer that a
/// call on another thread can free mid-read; two threads asking at once crashed Keepr.)
fn mounted() -> Vec<libc::statfs> {
    let n = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if n <= 0 {
        return vec![];
    }
    // Room for a few more, in case something mounts in between.
    let cap = n as usize + 8;
    let mut v: Vec<libc::statfs> = vec![unsafe { std::mem::zeroed() }; cap];
    let got = unsafe { libc::getfsstat(v.as_mut_ptr(), (cap * std::mem::size_of::<libc::statfs>()) as libc::c_int, libc::MNT_NOWAIT) };
    v.truncate(got.max(0) as usize);
    v
}

/// One mounted SMB share: its server as mounted ("192.168.1.20", "keep-nas"), the share, and
/// where it is.
struct SmbMount {
    host: String,
    share: String,
    on: PathBuf,
}

/// "//user@host/share" or "//host/share", as getmntinfo names a share, split into host and share.
/// Bonjour hosts look like "name._smb._tcp.local"; only the name is kept.
fn split_from(from: &str) -> Option<(String, String)> {
    let from = decode(from);
    let rest = from.trim_start_matches('/');
    let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
    let (host, share) = rest.split_once('/')?;
    Some((host.split("._smb._tcp").next().unwrap_or(host).to_string(), share.trim_matches('/').to_string()))
}

/// The SMB shares mounted now.
fn smb_mounts() -> Vec<SmbMount> {
    let text = |c: &[libc::c_char]| unsafe { CStr::from_ptr(c.as_ptr()) }.to_string_lossy().to_string();
    mounted()
        .iter()
        .filter(|m| text(&m.f_fstypename) == "smbfs")
        .filter_map(|m| {
            split_from(&text(&m.f_mntfromname)).map(|(host, share)| SmbMount { host, share, on: PathBuf::from(text(&m.f_mntonname)) })
        })
        .collect()
}

/// Servers of SMB shares mounted now ("192.168.1.20", "keep-nas").
pub fn mounted_servers() -> Vec<String> {
    smb_mounts().into_iter().map(|m| m.host).collect()
}

/// Where `server`'s `share` is mounted already, if it is.
pub fn find_mount(server: &str, share: &str) -> Option<PathBuf> {
    let (want_host, want_share) = (norm_host(server), share.trim_matches('/').to_lowercase());
    smb_mounts().into_iter().find(|m| norm_host(&m.host) == want_host && m.share.to_lowercase() == want_share).map(|m| m.on)
}

fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Mounts the share, returning where. The password is passed to NetFS directly, never on a
/// command line. No dialog is shown: an unattended backup can't answer one.
pub fn mount(server: &str, share: &str, user: &str, password: &str) -> Result<PathBuf, String> {
    let url = format!("smb://{}/{}", server.trim(), percent(share.trim_matches('/')));
    let url_cf = unsafe {
        let s = CFString::new(&url);
        let u = core_foundation_sys::url::CFURLCreateWithString(std::ptr::null(), s.as_concrete_TypeRef(), std::ptr::null());
        if u.is_null() {
            return Err(format!("{url} isn't a valid address"));
        }
        CFURL::wrap_under_create_rule(u)
    };
    let user_cf = CFString::new(user);
    let pw_cf = CFString::new(password);
    let mut open: CFMutableDictionary<CFString, CFType> = CFMutableDictionary::new();
    open.set(CFString::new("UIOption"), CFString::new("NoUI").as_CFType());
    let mut mnt: CFMutableDictionary<CFString, CFType> = CFMutableDictionary::new();
    mnt.set(CFString::new("MountFlags"), CFNumber::from(MNT_DONTBROWSE).as_CFType());
    let mut points: CFArrayRef = std::ptr::null();
    let rc = unsafe {
        NetFSMountURLSync(
            url_cf.as_concrete_TypeRef(),
            std::ptr::null(),
            if user.is_empty() { std::ptr::null() } else { user_cf.as_concrete_TypeRef() },
            if password.is_empty() { std::ptr::null() } else { pw_cf.as_concrete_TypeRef() },
            open.as_concrete_TypeRef(),
            mnt.as_concrete_TypeRef(),
            &mut points,
        )
    };
    if rc != 0 {
        if let Some(p) = find_mount(server, share) {
            return Ok(p); // mounted meanwhile, or already (NetFS says EEXIST)
        }
        return Err(match rc {
            libc::EAUTH | libc::EACCES | libc::EPERM => format!("{server} didn't accept the name and password."),
            libc::ENOENT => format!("{server} has no share called {share}."),
            libc::EHOSTUNREACH | libc::ETIMEDOUT | libc::ENETUNREACH | 64 | -6600 | -6602 => {
                format!("{server} can't be reached. Is it on and on this network?")
            }
            _ => format!("Couldn't connect to {server}/{share} (error {rc})."),
        });
    }
    let arr: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(points) };
    arr.iter()
        .next()
        .map(|s| PathBuf::from(s.to_string()))
        .or_else(|| find_mount(server, share))
        .ok_or_else(|| format!("{server}/{share} mounted but can't be found"))
}

/// Unmounts a share Keepr mounted; false if it's still mounted (busy, say).
pub fn unmount(path: &std::path::Path) -> bool {
    let Ok(c) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else { return false };
    unsafe { libc::unmount(c.as_ptr(), 0) == 0 }
}

/// Servers offering SMB on this network, by their Bonjour names ("keep-nas"). Takes about a
/// second. Asks Bonjour directly (dns_sd): `dns-sd -B` buffers its output when it isn't writing to
/// a terminal, so reading it for a second gets nothing.
pub fn discover() -> Vec<String> {
    use std::ffi::{c_char, c_void, CStr};
    type BrowseReply = extern "C" fn(
        sd: *mut c_void,
        flags: u32,
        iface: u32,
        err: i32,
        name: *const c_char,
        regtype: *const c_char,
        domain: *const c_char,
        ctx: *mut c_void,
    );
    extern "C" {
        fn DNSServiceBrowse(
            sd: *mut *mut c_void,
            flags: u32,
            iface: u32,
            regtype: *const c_char,
            domain: *const c_char,
            cb: BrowseReply,
            ctx: *mut c_void,
        ) -> i32;
        fn DNSServiceRefSockFD(sd: *mut c_void) -> i32;
        fn DNSServiceProcessResult(sd: *mut c_void) -> i32;
        fn DNSServiceRefDeallocate(sd: *mut c_void);
    }
    extern "C" fn reply(
        _sd: *mut c_void,
        flags: u32,
        _i: u32,
        err: i32,
        name: *const c_char,
        _t: *const c_char,
        _d: *const c_char,
        ctx: *mut c_void,
    ) {
        const ADD: u32 = 0x2;
        if err == 0 && flags & ADD != 0 && !name.is_null() {
            let names = unsafe { &mut *(ctx as *mut Vec<String>) };
            names.push(unsafe { CStr::from_ptr(name) }.to_string_lossy().to_string());
        }
    }
    let mut names: Vec<String> = Vec::new();
    unsafe {
        let mut sd: *mut c_void = std::ptr::null_mut();
        if DNSServiceBrowse(&mut sd, 0, 0, c"_smb._tcp".as_ptr(), c"local.".as_ptr(), reply, &mut names as *mut Vec<String> as *mut c_void)
            != 0
        {
            return vec![];
        }
        let fd = DNSServiceRefSockFD(sd);
        let end = std::time::Instant::now() + std::time::Duration::from_millis(1200);
        while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
            let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            if libc::poll(&mut p, 1, left.as_millis() as i32) <= 0 {
                break;
            }
            if DNSServiceProcessResult(sd) != 0 {
                break;
            }
        }
        DNSServiceRefDeallocate(sd);
    }
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// The shares a server offers (not the hidden ones ending "$"), asking with the name and password.
/// `smbutil view` reads the password from its terminal, so it gets one: a pseudo-terminal Keepr
/// types the password into. It never appears on a command line.
pub fn shares(server: &str, user: &str, password: &str) -> Result<Vec<String>, String> {
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    let (mut master, mut slave) = (0, 0);
    if unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) } != 0 {
        return Err("Couldn't ask the server for its shares.".into());
    }
    // Owned at once, so every way out of here closes them.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    let target = if user.is_empty() { format!("//{server}") } else { format!("//{}@{server}", percent(user)) };
    let stdio = || slave.try_clone().map(std::process::Stdio::from).map_err(|e| e.to_string());
    let mut cmd = std::process::Command::new("/usr/bin/smbutil");
    cmd.arg("view");
    // Options before the server: smbutil stops reading options at the first other argument.
    if user.is_empty() {
        cmd.arg("-N");
    }
    cmd.arg(&target).stdin(stdio()?).stdout(stdio()?).stderr(stdio()?);
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            libc::ioctl(0, libc::TIOCSCTTY as _, 0);
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    // The command holds copies of the terminal's other end; only smbutil should have it now.
    drop(cmd);
    drop(slave);
    let mut m = std::fs::File::from(master);
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let mut reader = match m.try_clone() {
        Ok(r) => r,
        Err(e) => {
            // Left alone, smbutil would sit at its password prompt for ever.
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.to_string());
        }
    };
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut out = String::new();
    let mut sent = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(b) => out.push_str(&String::from_utf8_lossy(&b)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
        if !sent && out.to_lowercase().contains("password") {
            let _ = m.write_all(format!("{password}\n").as_bytes());
            sent = true;
        }
        if let Ok(Some(_)) = child.try_wait() {
            // Let the last output arrive.
            while let Ok(b) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
                out.push_str(&String::from_utf8_lossy(&b));
            }
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let names = parse_shares(&out);
    if names.is_empty() {
        // smbutil's own prompt says "Password", so it can't be the sign; its failure messages are.
        let low = out.to_lowercase();
        return Err(if ["authentication error", "logon failure", "permission denied"].iter().any(|t| low.contains(t)) {
            format!("{server} didn't accept the name and password.")
        } else {
            format!("{server} didn't list any shares.")
        });
    }
    Ok(names)
}

fn parse_shares(out: &str) -> Vec<String> {
    let mut names: Vec<String> = out
        .lines()
        .filter_map(|l| {
            let l = l.trim_end_matches('\r');
            let i = l.find(" Disk")?;
            let name = l[..i].trim();
            (!name.is_empty() && !name.ends_with('$')).then(|| name.to_string())
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

#[cfg(test)]
mod tests {
    /// Against a real server: KEEPR_TEST_SMB_HOST=192.168.1.20 cargo test smb -- --ignored
    #[test]
    #[ignore]
    fn lists_a_real_servers_shares_as_guest() {
        let host = std::env::var("KEEPR_TEST_SMB_HOST").expect("KEEPR_TEST_SMB_HOST");
        let s = super::shares(&host, "", "").unwrap();
        assert!(!s.is_empty());
        println!("shares: {s:?}; servers: {:?}", super::discover());
    }

    #[test]
    fn parses_share_lists() {
        let out = "Password for keep-nas:\r\nShare                                           Type    Comments\n-------------------------------\nBackups                                         Disk    \nMy Photos                                       Disk    family\nIPC$                                            Pipe    IPC Service\nADMIN$                                          Disk    \n\n3 shares listed\n";
        assert_eq!(super::parse_shares(out), vec!["Backups", "My Photos"]);
    }

    use super::*;

    #[test]
    fn decodes_mount_names() {
        assert_eq!(decode("//wayne@keep-nas._smb._tcp.local/My%20Share"), "//wayne@keep-nas._smb._tcp.local/My Share");
        assert_eq!(norm_host("Keep-NAS.local."), "keep-nas");
        assert_eq!(percent("My Share"), "My%20Share");
        // A "%" before a character of more than one byte is kept as it is, not a panic.
        assert_eq!(decode("//h/50%é%"), "//h/50%é%");
        assert_eq!(decode("%C3%A9"), "é");
        assert_eq!(split_from("//wayne@keep-nas._smb._tcp.local/My%20Share/"), Some(("keep-nas".to_string(), "My Share".to_string())));
        assert_eq!(split_from("//192.168.1.20/Backups"), Some(("192.168.1.20".to_string(), "Backups".to_string())));
    }
}
