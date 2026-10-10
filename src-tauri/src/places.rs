// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

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
        // A share that won't unmount (a file still open on it) stays on the list, to be tried again
        // next time, rather than being forgotten while it's still mounted and hidden from Finder.
        for p in done {
            if smb::unmount(&p) {
                m.remove(&p);
            }
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

/// In a cloud service's sync folder (OneDrive, Google Drive, iCloud Drive…).
pub fn in_cloud(p: &Path) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    cloud_root(p).is_some() || p.starts_with(Path::new(&home).join("Library/Mobile Documents"))
}

/// For a folder in a cloud service's sync folder (~/Library/CloudStorage/OneDrive-Personal/…):
/// that sync folder, and whether macOS still treats it as a live one. When a cloud account is
/// signed out or its sync root breaks, macOS renames the root and leaves its contents looking
/// intact; only the file-provider domain attribute tells a live root from an orphan. Backing up
/// into an orphan would quietly go to local disk and never reach the cloud.
pub fn cloud_root(p: &Path) -> Option<(PathBuf, bool)> {
    let home = std::env::var("HOME").ok()?;
    let cs = Path::new(&home).join("Library/CloudStorage");
    let rest = p.strip_prefix(&cs).ok()?;
    let root = cs.join(rest.components().next()?);
    let c = std::ffi::CString::new(root.to_string_lossy().as_bytes()).ok()?;
    let name = c"com.apple.file-provider-domain-id";
    let n = unsafe { libc::getxattr(c.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
    Some((root, n > 0))
}

/// "ContosoHoldingsLimited" → "Contoso Holdings Limited".
fn words(s: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = s.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| cs[j]);
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase()) {
            out.push(' ');
        }
        out.push(if c == '_' || c == '-' { ' ' } else { c });
    }
    out
}

/// A cloud sync folder's name as people know it: "OneDrive-Personal" → "OneDrive Personal",
/// "iCloudDrive-iCloudDrive" → "iCloud Drive", "GoogleDrive-me@gmail.com" → "Google Drive me@gmail.com".
pub fn cloud_name(root: &str) -> String {
    let root = root.split(" (").next().unwrap_or(root);
    let (provider, account) = root.split_once('-').unwrap_or((root, ""));
    let p = match provider {
        "iCloudDrive" => "iCloud Drive".to_string(),
        "OneDrive" => "OneDrive".to_string(),
        other => words(other),
    };
    if account.is_empty() || account == provider {
        p
    } else if account.contains('@') {
        format!("{p} {account}")
    } else {
        format!("{p} {}", words(account))
    }
}

/// A name that says where a place is: "OneDrive Personal: Keepr", "Archive SSD: Keepr",
/// "keep-nas: Backups", or the folder's own name.
pub fn default_name(p: &Place) -> String {
    match p {
        Place::Folder { path, .. } => {
            let pb = Path::new(path);
            let leaf = pb.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Folder".into());
            if let Some((root, _)) = cloud_root(pb) {
                let r = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let cloud = cloud_name(&r);
                return if root == pb { cloud } else { format!("{cloud}: {leaf}") };
            }
            let home = std::env::var("HOME").unwrap_or_default();
            let icloud = Path::new(&home).join("Library/Mobile Documents/com~apple~CloudDocs");
            if pb.starts_with(&icloud) {
                return if pb == icloud { "iCloud Drive".into() } else { format!("iCloud Drive: {leaf}") };
            }
            match volume_of(pb) {
                Some(vol) if vol != leaf => format!("{vol}: {leaf}"),
                Some(vol) => vol,
                None => leaf,
            }
        }
        Place::Smb(s) => format!("{}: {}", s.server.trim_end_matches(".local"), s.share.trim_matches('/')),
        Place::S3(s) => format!("{}: {}", s3_service(&s.endpoint), s.bucket.trim()),
    }
}

/// "Amazon S3", "Cloudflare R2", "Wasabi", or the endpoint's host.
pub fn s3_service(endpoint: &str) -> String {
    let host = endpoint.split("://").last().unwrap_or_default().split(['/', ':']).next().unwrap_or_default().to_lowercase();
    if host.ends_with(".amazonaws.com") {
        "Amazon S3".into()
    } else if host.ends_with(".backblazeb2.com") {
        "Backblaze B2".into()
    } else if host.ends_with(".r2.cloudflarestorage.com") {
        "Cloudflare R2".into()
    } else if host.ends_with(".wasabisys.com") {
        "Wasabi".into()
    } else {
        host
    }
}

/// What sort of place a destination is, for its icon ("folder", "drive", "cloud", "smb", "s3")
/// and its tooltip ("OneDrive Personal", "External drive").
pub fn kind_of(p: &Place) -> (&'static str, String) {
    match p {
        Place::Folder { path, .. } => {
            let pb = Path::new(path);
            if let Some((root, _)) = cloud_root(pb) {
                return ("cloud", cloud_name(&root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            }
            let home = std::env::var("HOME").unwrap_or_default();
            if pb.starts_with(Path::new(&home).join("Library/Mobile Documents/com~apple~CloudDocs")) {
                return ("cloud", "iCloud Drive".into());
            }
            match volume_of(pb) {
                Some(_) => ("drive", "External drive".into()),
                None => ("folder", "Folder on this Mac".into()),
            }
        }
        Place::Smb(_) => ("smb", "SMB share".into()),
        Place::S3(s) => ("s3", s3_service(&s.endpoint)),
    }
}

/// A bucket's storage, `within` a plan's folder in it.
pub fn s3_backend(s: &crate::config::S3, secret: Option<String>, within: &str) -> Result<keepr_engine::s3::S3, String> {
    let secret = secret
        .filter(|x| !x.is_empty())
        .or_else(|| keychain::get(&keychain::s3_account(&s.access_key)))
        .ok_or("The secret key for this bucket isn't in the Keychain. Enter it in the destination.")?;
    let b = keepr_engine::s3::S3::new(keepr_engine::s3::Config {
        endpoint: s.endpoint.clone(),
        region: s.region.clone(),
        bucket: s.bucket.clone(),
        prefix: s.prefix.clone(),
        access_key: s.access_key.trim().to_string(),
        secret_key: secret,
    })
    .map_err(|e| e.to_string())?;
    Ok(b.within(within))
}

/// A place's own name if it has one, else its default.
pub fn name_of(p: &Place) -> String {
    let own = match p {
        Place::Folder { name, .. } => name.as_deref(),
        Place::Smb(s) => s.name.as_deref(),
        Place::S3(s) => s.name.as_deref(),
    };
    own.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| default_name(p))
}

/// `base`, or "base 2", "base 3"… if that's taken (any case).
pub fn unique_name(base: &str, taken: &[String]) -> String {
    let free = |n: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(n));
    if free(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| free(n)).unwrap()
}

/// A cloud service's sync folder on this Mac, as the destination picker lists it.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CloudFolder {
    /// "OneDrive Personal", "iCloud Drive".
    pub name: String,
    /// "onedrive", "icloud", "google", "dropbox" or "other".
    pub provider: String,
    pub root: String,
    /// False when it's there but not a working sync folder (iCloud Drive turned off, or an
    /// orphaned root); such folders are listed so people see why they can't be picked.
    pub live: bool,
    pub why: String,
    pub free: Option<u64>,
}

fn provider_of(root: &str) -> &'static str {
    let r = root.to_lowercase();
    if r.starts_with("onedrive") {
        "onedrive"
    } else if r.starts_with("googledrive") {
        "google"
    } else if r.starts_with("dropbox") {
        "dropbox"
    } else if r.starts_with("icloud") {
        "icloud"
    } else {
        "other"
    }
}

/// The cloud services' sync folders on this Mac: everything in ~/Library/CloudStorage that macOS
/// treats as a live sync root, and iCloud Drive (~/Library/Mobile Documents/com~apple~CloudDocs,
/// which only exists while iCloud Drive is on).
pub fn cloud_folders() -> Vec<CloudFolder> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();
    let icloud = Path::new(&home).join("Library/Mobile Documents/com~apple~CloudDocs");
    out.push(CloudFolder {
        name: "iCloud Drive".into(),
        provider: "icloud".into(),
        root: icloud.to_string_lossy().to_string(),
        live: icloud.is_dir(),
        why: if icloud.is_dir() {
            String::new()
        } else {
            "iCloud Drive is turned off (System Settings › Apple Account › iCloud)".into()
        },
        free: space(&icloud).map(|s| s.0),
    });
    if let Ok(rd) = std::fs::read_dir(Path::new(&home).join("Library/CloudStorage")) {
        let mut found: Vec<CloudFolder> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let dir = e.file_name().to_string_lossy().to_string();
                let provider = provider_of(&dir);
                // The old iCloud Drive location is only ever an orphan now.
                if provider == "icloud" {
                    return None;
                }
                let (_, live) = cloud_root(&e.path().join("x"))?;
                (live).then(|| CloudFolder {
                    name: cloud_name(&dir),
                    provider: provider.into(),
                    root: e.path().to_string_lossy().to_string(),
                    live,
                    why: String::new(),
                    free: space(&e.path()).map(|s| s.0),
                })
            })
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(found);
    }
    out
}

/// A share's password: Keepr's saved one, else Finder's.
pub fn smb_password(s: &Smb) -> Option<String> {
    keychain::get(&keychain::smb_account(&s.user, &s.server)).or_else(|| keychain::finder_smb_password(&s.server, &s.user))
}

pub fn describe(p: &Place) -> String {
    match p {
        Place::Folder { path, .. } => tilde(path),
        Place::Smb(s) => format!(
            "smb://{}/{}{}",
            s.server,
            s.share,
            if s.folder.trim_matches('/').is_empty() { String::new() } else { format!("/{}", s.folder.trim_matches('/')) }
        ),
        Place::S3(s) => format!(
            "s3://{}{}",
            s.bucket.trim(),
            if s.prefix.trim_matches('/').is_empty() { String::new() } else { format!("/{}", s.prefix.trim_matches('/')) }
        ),
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
    resolve_with(place, mounts, connect, disconnect_after, None)
}

/// As `resolve`, connecting a share with `password` rather than the saved one when it's given
/// (testing a destination before its password is saved). A share mounted this way is noted like
/// any other, so it is disconnected again once it's idle.
pub fn resolve_with(
    place: &Place,
    mounts: &Mounts,
    connect: bool,
    disconnect_after: bool,
    password: Option<&str>,
) -> Result<PathBuf, String> {
    match place {
        Place::Folder { path, .. } => {
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
            if let Some((root, live)) = cloud_root(&p) {
                let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if !root.exists() || !live {
                    return Err(format!("{name} isn't a working sync folder right now (is the cloud account signed in?), so nothing written there would reach the cloud."));
                }
            }
            // The folder must be where it was: a vanished symlink or a folder made in its place
            // would put backups somewhere else.
            let real = std::fs::canonicalize(&p).map_err(|_| format!("{} isn't there.", tilde(path)))?;
            if let (Some(_), None) = (cloud_root(&p), cloud_root(&real)) {
                return Err(format!("{} no longer leads into its cloud sync folder.", tilde(path)));
            }
            Ok(p)
        }
        Place::Smb(s) => {
            let base = match smb::find_mount(&s.server, &s.share) {
                Some(m) => m,
                None if connect => {
                    let pw = password.map(str::to_string).or_else(|| smb_password(s)).unwrap_or_default();
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
        Place::S3(s) => Err(format!("{} is a bucket, not a folder.", describe(&Place::S3(s.clone())))),
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
        let e = resolve(&Place::Folder { path: "/Volumes/Keepr Test Drive That Isn't There/x".into(), name: None }, &m, true, true)
            .unwrap_err();
        assert!(e.contains("isn't connected"), "{e}");
        let t = tempfile::tempdir().unwrap();
        assert!(resolve(&Place::Folder { path: t.path().to_string_lossy().into(), name: None }, &m, true, true).is_ok());
        assert_eq!(mount_point(Path::new("/")).as_deref(), Some(Path::new("/")));
        assert!(cloud_root(Path::new("/Users/nobody/Documents")).is_none());
        assert_eq!(cloud_name("OneDrive-ContosoHoldingsLimited"), "OneDrive Contoso Holdings Limited");
        assert_eq!(cloud_name("OneDrive-Personal"), "OneDrive Personal");
        assert_eq!(cloud_name("iCloudDrive-iCloudDrive (2024-03-26 10:00)"), "iCloud Drive");
        assert_eq!(cloud_name("GoogleDrive-me@gmail.com"), "Google Drive me@gmail.com");
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            default_name(&Place::Folder { path: format!("{home}/Library/CloudStorage/OneDrive-Personal/Keepr"), name: None }),
            "OneDrive Personal: Keepr"
        );
        assert_eq!(default_name(&Place::Folder { path: "/Volumes/Archive SSD/Keepr".into(), name: None }), "Archive SSD: Keepr");
        assert_eq!(default_name(&Place::Folder { path: "/Volumes/Archive SSD".into(), name: None }), "Archive SSD");
        assert_eq!(unique_name("Keepr", &["keepr".into(), "Keepr 2".into()]), "Keepr 3");
        assert_eq!(
            default_name(&Place::Folder { path: format!("{home}/Library/Mobile Documents/com~apple~CloudDocs/Keepr"), name: None }),
            "iCloud Drive: Keepr"
        );
        let found = cloud_folders();
        assert_eq!(found[0].provider, "icloud");
        assert!(found.iter().skip(1).all(|c| c.live && c.provider != "icloud"));
        println!("cloud folders: {:?}", found.iter().map(|c| (&c.name, c.live)).collect::<Vec<_>>());
        let home = std::env::var("HOME").unwrap();
        let (root, _) = cloud_root(&Path::new(&home).join("Library/CloudStorage/Some-Cloud/Backups/Keepr")).unwrap();
        assert!(root.ends_with("Library/CloudStorage/Some-Cloud"));
        let e = resolve(&Place::Folder { path: format!("{home}/Library/CloudStorage/Keepr-Test-Not-There/x"), name: None }, &m, true, true)
            .unwrap_err();
        assert!(e.contains("sync folder"), "{e}");
    }
}
