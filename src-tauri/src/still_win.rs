// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! A still copy of a drive for the length of a backup on Windows, as still.rs takes one on macOS:
//! a shadow copy (Volume Shadow Copy) made by Keepr Helper (helper_win.rs), read through its device
//! path (\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy7\Users\w\Documents). Every file is read as
//! it was at one moment. The copy lasts as long as the connection it was made on: dropping the
//! Still deletes it, and so does Keepr ending however it ends.
//!
//! For folders on one local NTFS drive, while the helper is installed and running. Without it
//! files are read as they are.

use crate::helper_win::{self, Conn};
use serde_json::json;
use std::path::{Path, PathBuf};

pub struct Still {
    pub date: String,
    /// The shadow copy's device, which stands for the drive's root.
    pub mount: PathBuf,
    /// The drive it's a copy of: "C:\".
    root: String,
    id: String,
    conn: Conn,
}

/// The drive a path is on, as its root ("C:\"), when it is a drive letter's.
fn drive_root(p: &Path) -> Option<String> {
    let s = keepr_engine::paths::native(&p.to_string_lossy()).to_string_lossy().into_owned();
    let b = s.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':').then(|| format!("{}:\\", s[..1].to_uppercase()))
}

/// Whether `p` is somewhere a still copy can be taken: a fixed NTFS drive, with the helper there.
pub fn on_data_volume(p: &Path) -> bool {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW};
    let Some(root) = drive_root(p) else { return false };
    let r = windows::core::HSTRING::from(root.as_str());
    // DRIVE_FIXED (3): not a USB stick or a network drive.
    if unsafe { GetDriveTypeW(&r) } != 3 {
        return false;
    }
    let mut fs = [0u16; 32];
    if unsafe { GetVolumeInformationW(&r, None, None, None, None, Some(&mut fs)) }.is_err() {
        return false;
    }
    let n = fs.iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf16_lossy(&fs[..n]) == "NTFS" && helper_win::available()
}

/// Whether the folders are all on one drive: a still copy and a change journal are a drive's.
pub fn one_volume(paths: &[PathBuf]) -> bool {
    let roots: std::collections::HashSet<Option<String>> = paths.iter().map(|p| drive_root(p)).collect();
    roots.len() == 1 && !roots.contains(&None)
}

/// Where the copy has `p`: C:\Users\w\Documents → <device>\Users\w\Documents.
pub fn inside(still: &Still, p: &Path) -> PathBuf {
    let native = keepr_engine::paths::native(&p.to_string_lossy());
    let s = native.to_string_lossy();
    let rest =
        if s.len() >= still.root.len() && s[..still.root.len()].eq_ignore_ascii_case(&still.root) { &s[still.root.len()..] } else { "" };
    if rest.is_empty() {
        still.mount.clone()
    } else {
        still.mount.join(rest)
    }
}

pub fn take(roots: &[PathBuf]) -> Result<Still, String> {
    let root = roots.first().and_then(|p| drive_root(p)).ok_or("the folders aren't on a drive")?;
    let mut conn = Conn::open()?;
    let v = conn.call(json!({ "op": "snapshot", "volume": root }))?;
    let (id, device) = (v["id"].as_str().unwrap_or_default().to_string(), v["device"].as_str().unwrap_or_default().to_string());
    // Read through a path that ends in the separator, as the device's root.
    let mount = PathBuf::from(format!("{device}\\"));
    Ok(Still { date: format!("{} {}", root.trim_end_matches('\\'), chrono::Local::now().format("%H:%M:%S")), mount, root, id, conn })
}

impl Drop for Still {
    fn drop(&mut self) {
        let _ = self.conn.call(json!({ "op": "release", "id": self.id }));
    }
}

/// Nothing to clean up: the helper deletes copies whose connection went (still.rs's are mounted
/// and left behind by a Keepr that stopped mid-backup).
pub fn clean_up_left_behind() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paths_into_the_copy() {
        assert_eq!(drive_root(Path::new("D:/Photos")).as_deref(), Some("D:\\"));
        assert!(one_volume(&[PathBuf::from("/Users/w/A"), PathBuf::from("/Users/w/B")]));
        assert!(!one_volume(&[PathBuf::from("/Users/w/A"), PathBuf::from("D:/B")]));
        assert!(!one_volume(&[]));
    }
}
