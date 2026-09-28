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
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Where `server`'s `share` is mounted already, if it is.
pub fn find_mount(server: &str, share: &str) -> Option<PathBuf> {
    let (want_host, want_share) = (norm_host(server), share.trim_matches('/').to_lowercase());
    let mut buf: *mut libc::statfs = std::ptr::null_mut();
    let n = unsafe { libc::getmntinfo(&mut buf, libc::MNT_NOWAIT) };
    for i in 0..n.max(0) as usize {
        let m = unsafe { &*buf.add(i) };
        let fstype = unsafe { CStr::from_ptr(m.f_fstypename.as_ptr()) }.to_string_lossy();
        if fstype != "smbfs" {
            continue;
        }
        let from = decode(&unsafe { CStr::from_ptr(m.f_mntfromname.as_ptr()) }.to_string_lossy());
        // "//user@host/share" or "//host/share"; Bonjour hosts look like "name._smb._tcp.local".
        let rest = from.trim_start_matches('/');
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        let Some((host, sh)) = rest.split_once('/') else { continue };
        let host = norm_host(host.split("._smb._tcp").next().unwrap_or(host));
        if host == want_host && sh.trim_matches('/').to_lowercase() == want_share {
            return Some(PathBuf::from(unsafe { CStr::from_ptr(m.f_mntonname.as_ptr()) }.to_string_lossy().to_string()));
        }
    }
    None
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
            libc::EHOSTUNREACH | libc::ETIMEDOUT | libc::ENETUNREACH | 64 | -6600 | -6602 => format!("{server} can't be reached. Is it on and on this network?"),
            _ => format!("Couldn't connect to {server}/{share} (error {rc})."),
        });
    }
    let arr: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(points) };
    arr.iter().next().map(|s| PathBuf::from(s.to_string())).or_else(|| find_mount(server, share)).ok_or_else(|| format!("{server}/{share} mounted but can't be found"))
}

pub fn unmount(path: &std::path::Path) {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap_or_default();
    unsafe {
        libc::unmount(c.as_ptr(), 0);
    }
}

/// Servers offering SMB on this network, by their Bonjour names ("keep-nas"). Takes about a second.
pub fn discover() -> Vec<String> {
    let Ok(mut child) = std::process::Command::new("/usr/bin/dns-sd").args(["-B", "_smb._tcp", "local."]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn() else {
        return vec![];
    };
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let _ = child.kill();
    let out = child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let mut names: Vec<String> = out
        .lines()
        .filter(|l| l.contains(" Add ") && l.contains("_smb._tcp."))
        .filter_map(|l| l.split("_smb._tcp.").nth(1).map(|n| n.trim().to_string()))
        .filter(|n| !n.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_mount_names() {
        assert_eq!(decode("//wayne@keep-nas._smb._tcp.local/My%20Share"), "//wayne@keep-nas._smb._tcp.local/My Share");
        assert_eq!(norm_host("Keep-NAS.local."), "keep-nas");
        assert_eq!(percent("My Share"), "My%20Share");
    }
}
