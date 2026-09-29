// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checking a repository: that every snapshot's trees load, that every chunk they name is in the
//! index and its pack exists, and (for a share of the packs) that the data reads back and matches
//! its id.

use crate::backup::Control;
use crate::repo::{pack_path, Repo};
use crate::tree::NodeKind;
use crate::{Id, Result};
use rand::seq::SliceRandom;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::Ordering::Relaxed;

#[derive(Default, Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub snapshots: u64,
    pub trees: u64,
    pub chunks: u64,
    pub packs: u64,
    pub packs_read: u64,
    pub bytes_read: u64,
    /// Empty when everything is intact.
    pub problems: Vec<String>,
}

impl Report {
    fn problem(&mut self, p: String) {
        if self.problems.len() < 100 {
            self.problems.push(p);
        }
    }
}

/// `read_share` of the packs (0.0 to 1.0, at least one when above 0) are read in full.
pub fn run(repo: &Repo, read_share: f64, ctl: &Control) -> Result<Report> {
    let mut rep = Report::default();
    let snaps = repo.snapshots()?;
    rep.snapshots = snaps.len() as u64;

    let mut seen: HashSet<Id> = HashSet::new();
    let mut chunks: HashSet<Id> = HashSet::new();
    for s in &snaps {
        let mut stack = vec![s.tree];
        while let Some(t) = stack.pop() {
            if !seen.insert(t) {
                continue;
            }
            ctl.checkpoint()?;
            rep.trees += 1;
            let tree = match repo.load_tree(&t) {
                Ok(t) => t,
                Err(e) => {
                    rep.problem(format!("The snapshot from {} can't read a folder: {e}", s.time));
                    continue;
                }
            };
            for n in &tree.nodes {
                match n.kind {
                    NodeKind::Dir => stack.extend(n.subtree),
                    NodeKind::File => {
                        for c in &n.content {
                            if chunks.insert(*c) && !repo.has(c) {
                                rep.problem(format!("{} in the snapshot from {} is missing data.", n.name, s.time));
                            }
                        }
                    }
                    NodeKind::Symlink => {}
                }
            }
        }
    }
    rep.chunks = chunks.len() as u64;

    let (packs, by_pack) = {
        let ix = repo.index.read().unwrap();
        let mut by_pack: Vec<Vec<Id>> = vec![Vec::new(); ix.packs.len()];
        for (id, loc) in &ix.blobs {
            by_pack[loc.pack as usize].push(*id);
        }
        (ix.packs.clone(), by_pack)
    };
    rep.packs = packs.len() as u64;
    let on_disk: HashSet<String> = repo.backend.list("packs")?.into_iter().map(|f| format!("packs/{f}")).collect();
    for p in &packs {
        if !on_disk.contains(&pack_path(p)) {
            rep.problem(format!("A pack of data is missing ({}).", p.short()));
        }
    }

    if read_share > 0.0 && !packs.is_empty() {
        let mut order: Vec<usize> = (0..packs.len()).collect();
        order.shuffle(&mut rand::thread_rng());
        let n = ((packs.len() as f64 * read_share).ceil() as usize).clamp(1, packs.len());
        ctl.progress.files_to_read.store(n as u64, Relaxed);
        for &i in &order[..n] {
            ctl.checkpoint()?;
            for id in &by_pack[i] {
                match repo.load(id) {
                    Ok(d) => {
                        rep.bytes_read += d.len() as u64;
                        ctl.progress.bytes_read.fetch_add(d.len() as u64, Relaxed);
                    }
                    Err(e) => rep.problem(format!("Damaged data in pack {}: {e}", packs[i].short())),
                }
            }
            rep.packs_read += 1;
            ctl.progress.files_read.fetch_add(1, Relaxed);
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{self, tests::*};
    use std::fs;

    #[test]
    fn finds_damage() {
        let (src, dst, repo) = setup(Some("pw"));
        fs::write(src.path().join("f"), vec![5u8; 50_000]).unwrap();
        backup::run(&repo, &opts(src.path()), None, &Control::default()).unwrap();
        assert!(run(&repo, 1.0, &Control::default()).unwrap().problems.is_empty());
        // Flip a byte in every pack.
        for f in repo.backend.list("packs").unwrap() {
            let p = dst.path().join("packs").join(f);
            let mut b = fs::read(&p).unwrap();
            b[30] ^= 0xff;
            fs::write(&p, b).unwrap();
        }
        let r = run(&repo, 1.0, &Control::default()).unwrap();
        assert!(!r.problems.is_empty());
    }
}
