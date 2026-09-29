// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! A still copy of the startup disk for the length of a backup: an APFS local snapshot, taken
//! with `tmutil localsnapshot` and mounted read-only (hidden from Finder). Every file is read as
//! it was at one moment, so a document saved halfway through a backup can't be half old, half new.
//!
//! This works for folders on the startup disk's Data volume (Documents, Projects…). Folders on
//! other drives and shares are read as they are. Keepr deletes the snapshot when the backup
//! ends, and at the next launch deletes any it left behind; it never touches Time Machine's own.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const DATA: &str = "/System/Volumes/Data";

pub struct Still {
    pub date: String,
    pub mount: PathBuf,
}

/// Whether `p` is on the startup disk's Data volume, where a still copy can be taken.
pub fn on_data_volume(p: &Path) -> bool {
    crate::places::mount_point(p).is_some_and(|m| m == Path::new(DATA))
}

/// Where the snapshot has `p`: /Users/w/Documents → <mount>/Users/w/Documents.
pub fn inside(still: &Still, p: &Path) -> PathBuf {
    let rel = p.strip_prefix(DATA).or_else(|_| p.strip_prefix("/")).unwrap_or(p);
    still.mount.join(rel)
}

fn parse_date(out: &str) -> Option<String> {
    let d = out.split("date:").nth(1)?.trim();
    (d.len() >= 17 && d.chars().all(|c| c.is_ascii_digit() || c == '-')).then(|| d.to_string())
}

fn stills_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Caches/Keepr/still")
}

pub fn take() -> Result<Still, String> {
    let out = Command::new("/usr/bin/tmutil").arg("localsnapshot").output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let date = parse_date(&text).ok_or_else(|| format!("macOS didn't make a snapshot: {}", text.trim()))?;
    let mount = stills_dir().join(&date);
    let _ = std::fs::create_dir_all(&mount);
    let st = Command::new("/sbin/mount_apfs")
        .args(["-o", "rdonly,nobrowse", "-s", &format!("com.apple.TimeMachine.{date}.local"), DATA])
        .arg(&mount)
        .output()
        .map_err(|e| e.to_string())?;
    let still = Still { date, mount };
    if !st.status.success() {
        let msg = String::from_utf8_lossy(&st.stderr).trim().to_string();
        drop_still(&still);
        return Err(format!("The snapshot couldn't be opened: {msg}"));
    }
    Ok(still)
}

pub fn drop_still(s: &Still) {
    let _ = Command::new("/sbin/umount").arg(&s.mount).output();
    let _ = std::fs::remove_dir(&s.mount);
    let _ = Command::new("/usr/bin/tmutil").args(["deletelocalsnapshots", &s.date]).output();
}

impl Drop for Still {
    fn drop(&mut self) {
        drop_still(self);
    }
}

/// Snapshots left mounted by a Keepr that stopped mid-backup.
pub fn clean_up_left_behind() {
    let Ok(rd) = std::fs::read_dir(stills_dir()) else { return };
    for e in rd.flatten() {
        let date = e.file_name().to_string_lossy().to_string();
        if parse_date(&format!("date: {date}")).is_some() {
            drop(Still { date, mount: e.path() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tmutil_and_maps_paths() {
        assert_eq!(parse_date("NOTE: …\nCreated local snapshot with date: 2026-09-28-173142\n").as_deref(), Some("2026-09-28-173142"));
        assert_eq!(parse_date("error"), None);
        let s = std::mem::ManuallyDrop::new(Still { date: "x".into(), mount: PathBuf::from("/tmp/m") });
        assert_eq!(inside(&s, Path::new("/Users/w/Documents")), PathBuf::from("/tmp/m/Users/w/Documents"));
        assert_eq!(inside(&s, Path::new("/System/Volumes/Data/Users/w")), PathBuf::from("/tmp/m/Users/w"));
    }
}
