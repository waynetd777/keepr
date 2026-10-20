// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What changed since the last backup on Windows, as fsevents.rs gives it on macOS: from the
//! drive's NTFS change journal, which Keepr Helper reads (helper_win.rs), since the point in it
//! noted at the last backup. Every other folder is taken from the last snapshot without being
//! looked at.
//!
//! A point is the journal's id and a position in it. The record isn't trusted when the journal
//! was made afresh, has dropped the records asked for, or the helper isn't there; then the backup
//! looks at everything. The weekly full backup re-reads everything regardless.

use crate::helper_win;
use serde_json::json;
use std::path::PathBuf;

fn drive(roots: &[PathBuf]) -> Option<String> {
    let s = keepr_engine::paths::native(&roots.first()?.to_string_lossy()).to_string_lossy().into_owned();
    let b = s.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':').then(|| format!("{}:\\", s[..1].to_uppercase()))
}

/// Where the journal of the drive `roots` are on is now: (its id, the position), noted before a
/// backup looks at anything, so what changes during it is the next one's.
pub fn now(roots: &[PathBuf]) -> Option<(u64, i64)> {
    if !helper_win::available() {
        return None;
    }
    let v = helper_win::call(json!({ "op": "journal", "volume": drive(roots)? })).ok()?;
    Some((v["journal"].as_str()?.parse().ok()?, v["next"].as_i64()?))
}

/// The folders under `roots` that changed since `since` in journal `journal`, or None if that
/// can't be trusted.
pub fn since(roots: &[PathBuf], journal: u64, since: i64) -> Option<keepr_engine::backup::Changes> {
    if roots.is_empty() || !helper_win::available() {
        return None;
    }
    let native: Vec<String> =
        roots.iter().map(|r| keepr_engine::paths::native(&r.to_string_lossy()).to_string_lossy().into_owned()).collect();
    let v = helper_win::call(
        json!({ "op": "changes", "volume": drive(roots)?, "journal": journal.to_string(), "since": since, "roots": native }),
    )
    .ok()?;
    if v["untrusted"].as_bool() == Some(true) {
        return None;
    }
    let dirs: Vec<PathBuf> = v["dirs"].as_array()?.iter().filter_map(|d| d.as_str()).map(PathBuf::from).collect();
    Some(keepr_engine::backup::Changes::from_events(dirs, vec![]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the installed Keepr Helper (as an administrator, or the user it was installed for):
    /// cargo test --lib helper_end_to_end -- --ignored
    #[test]
    #[ignore]
    fn helper_end_to_end() {
        assert!(helper_win::available(), "Keepr Helper isn't running");
        let dir = PathBuf::from(r"C:\src\kp-helper-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("quiet")).unwrap();
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        std::fs::write(dir.join("quiet/q.txt"), "q").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let roots = vec![PathBuf::from(keepr_engine::paths::app(&dir))];
        let (journal, at) = now(&roots).expect("the journal's position");

        // A still copy keeps the file as it was when it was taken.
        let still = crate::still::take(&roots).unwrap();
        std::fs::write(dir.join("a.txt"), "two").unwrap();
        let inside = crate::still::inside(&still, &roots[0]);
        assert_eq!(std::fs::read_to_string(inside.join("a.txt")).unwrap(), "one");
        assert!(std::fs::read_dir(&inside).unwrap().count() >= 2);
        drop(still);

        // The journal names the folder that changed, and not the one that didn't.
        std::fs::create_dir_all(dir.join("new")).unwrap();
        std::fs::write(dir.join("new/n.txt"), "n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let ch = since(&roots, journal, at).expect("changes since then");
        assert!(!ch.unchanged(&roots[0]), "{ch:?}");
        assert!(!ch.unchanged(&roots[0].join("new")), "{ch:?}");
        assert!(ch.unchanged(&roots[0].join("quiet")), "{ch:?}");
        // A position in another journal isn't trusted.
        assert!(since(&roots, journal ^ 1, at).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
