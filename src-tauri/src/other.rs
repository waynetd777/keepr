// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stand-ins, on a system that is neither macOS nor Windows, for the still copy (still.rs,
//! still_win.rs) and the record of changed folders (fsevents.rs, fsevents_win.rs), under the same
//! names, so core.rs calls the same functions everywhere. Each says there's nothing: files are
//! read as they are, and every backup compares every file with the last snapshot.

pub mod still {
    use std::path::{Path, PathBuf};

    pub struct Still {
        pub date: String,
    }

    pub fn on_data_volume(_p: &Path) -> bool {
        false
    }

    pub fn inside(_still: &Still, p: &Path) -> PathBuf {
        p.to_path_buf()
    }

    pub fn take(_roots: &[PathBuf]) -> Result<Still, String> {
        Err("there's no still copy on this system".into())
    }

    pub fn clean_up_left_behind() {}
}

pub mod fsevents {
    use std::path::PathBuf;

    pub fn now_id() -> u64 {
        0
    }

    pub fn since(_roots: &[PathBuf], _since: u64) -> Option<keepr_engine::backup::Changes> {
        None
    }
}
