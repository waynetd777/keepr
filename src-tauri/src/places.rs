//! Turning a source or destination into a folder on this Mac: checking a drive is really
//! connected, and connecting a share when it isn't.

use crate::config::{Place, Smb};
use crate::{keychain, smb};
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Shares Keepr mounted itself, and when each was last used.
#[derive(Default)]
pub struct Mounts(Mutex<HashMap<PathBuf, (Instant, bool)>>);

impl Mounts {
    fn note(&self, p: &Path, disconnect_after: bool) {
        self.0.lock().unwrap().insert(p.to_path_buf(), (Instant::now(), disconnect_after));
    }

    pub fn touch(&self, p: &Path) {
        if let Some(e) = self.0.lock().unwrap().iter_mut().find(|(m, _)| p.starts_with(m)) {
            e.1 .0 = Instant::now();
        }
    }

    /// Unmounts shares Keepr mounted that haven't been used for `idle`, unless told to keep them.
    pub fn reap(&self, idle: Duration) {
        let mut m = self.0.lock().unwrap();
        let done: Vec<PathBuf> = m.iter().filter(|(_, (t, disc))| *disc && t.elapsed() >= idle).map(|(p, _)| p.clone()).collect();
        for p in done {
            smb::unmount(&p);
            m.remove(&p);
        }
    }
}

/// The mount point a path is on ("/" for the startup disk).
pub fn mount_point(p: &Path) -> Option<PathBuf> {
    let c = std::ffi::CString::new(p.to_string_lossy().as_bytes()).ok()?;
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some(PathBuf::from(unsafe { CStr::from_ptr(s.f_mntonname.as_ptr()) }.to_string_lossy().to_string()))
}

/// Free and total bytes on the volume holding `p`.
pub fn space(p: &Path) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(p.to_string_lossy().as_bytes()).ok()?;
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some((s.f_bavail * s.f_bsize as u64, s.f_blocks * s.f_bsize as u64))
}

/// "/Volumes/Archive SSD/Keepr" → Some("Archive SSD").
fn volume_of(p: &Path) -> Option<String> {
    let mut c = p.components();
    if c.next()? != std::path::Component::RootDir || c.next()?.as_os_str() != "Volumes" {
        return None;
    }
    Some(c.next()?.as_os_str().to_string_lossy().to_string())
}

pub fn smb_password(s: &Smb) -> Option<String> {
    keychain::get(&keychain::smb_account(&s.user, &s.server))
}

pub fn describe(p: &Place) -> String {
    match p {
        Place::Folder { path } => tilde(path),
        Place::Smb(s) => format!("smb://{}/{}{}", s.server, s.share, if s.folder.trim_matches('/').is_empty() { String::new() } else { format!("/{}", s.folder.trim_matches('/')) }),
    }
}

pub fn tilde(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match p.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => p.to_string(),
    }
}

/// The folder a place is, connecting a share if `connect` and it isn't mounted.
pub fn resolve(place: &Place, mounts: &Mounts, connect: bool, disconnect_after: bool) -> Result<PathBuf, String> {
    match place {
        Place::Folder { path } => {
            let p = PathBuf::from(path);
            // A drive that isn't connected leaves /Volumes/<name> missing, or (worse) an empty
            // folder of that name on the startup disk. Writing there would fill the Mac's own
            // disk and look like a working backup.
            if let Some(vol) = volume_of(&p) {
                let root = Path::new("/Volumes").join(&vol);
                if mount_point(&root).is_none_or(|m| m != root) {
                    return Err(format!("{vol} isn't connected."));
                }
            }
            if !p.exists() {
                return Err(format!("{} isn't there.", tilde(path)));
            }
            Ok(p)
        }
        Place::Smb(s) => {
            let base = match smb::find_mount(&s.server, &s.share) {
                Some(m) => m,
                None if connect => {
                    let pw = smb_password(s).unwrap_or_default();
                    let m = smb::mount(&s.server, &s.share, &s.user, &pw)?;
                    mounts.note(&m, disconnect_after);
                    m
                }
                None => return Err(format!("{}/{} isn't connected.", s.server, s.share)),
            };
            mounts.touch(&base);
            let folder = s.folder.trim_matches('/');
            Ok(if folder.is_empty() { base } else { base.join(folder) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_unconnected_drives() {
        assert_eq!(volume_of(Path::new("/Volumes/Archive SSD/Keepr")).as_deref(), Some("Archive SSD"));
        assert_eq!(volume_of(Path::new("/Users/w")), None);
        let m = Mounts::default();
        let e = resolve(&Place::Folder { path: "/Volumes/Keepr Test Drive That Isn't There/x".into() }, &m, true, true).unwrap_err();
        assert!(e.contains("isn't connected"), "{e}");
        let t = tempfile::tempdir().unwrap();
        assert!(resolve(&Place::Folder { path: t.path().to_string_lossy().into() }, &m, true, true).is_ok());
        assert_eq!(mount_point(Path::new("/")).as_deref(), Some(Path::new("/")));
    }
}
