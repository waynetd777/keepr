// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! `Keepr --verify-restore <plan> <folder>`: restores a plan's latest snapshot into a folder
//! with the real restore, then compares every file byte for byte with its source. A difference
//! is only a fault when the source still looks as it did at the backup (same size and modified
//! time); otherwise the source changed afterwards, which is expected.
//!
//! Uses a frozen core: nothing is written to config.json or state.json, so it can run beside
//! the app.

use crate::core::{human_bytes, Core};
use keepr_engine::backup::Control;
use keepr_engine::restore::{self, Conflict, Target};
use keepr_engine::tree::{Node, NodeKind};
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[derive(Default)]
struct Tally {
    files: u64,
    bytes: u64,
    same: u64,
    changed_since: u64,
    deleted_since: u64,
    faults: Vec<String>,
    notes: Vec<String>,
}

pub fn run(plan: &str, dir: &Path) -> Result<bool, String> {
    let core = Core::new(crate::config::data_dir(), Box::new(|_, _| {}), Box::new(|_, _| {}), true);
    let name = core.config.lock().unwrap().plan(plan).map(|p| p.name.clone()).ok_or("no such plan")?;
    let repo = core.repo(plan, false)?;
    let snaps = repo.snapshots().map_err(|e| e.0)?;
    let snap = snaps.last().ok_or("no snapshots")?.clone();
    println!("{name}: snapshot {} from {}, {} sources", snap.id.short(), snap.time, snap.sources.len());
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    let r = restore::run(&repo, &snap, &snap.sources, &Target::Folder(dir.to_path_buf()), Conflict::Replace, &Control::default())
        .map_err(|e| e.0)?;
    println!(
        "{name}: restored {} files, {} in {:.0} s; {} errors",
        r.files,
        human_bytes(r.bytes),
        t.elapsed().as_secs_f64(),
        r.errors.len()
    );
    let mut tally = Tally::default();
    for e in &r.errors {
        tally.faults.push(format!("restore error: {e}"));
    }
    let root = repo.load_tree(&snap.tree).map_err(|e| e.0)?;
    for n in &root.nodes {
        let restored = restore::destination(&snap, &n.name, &Target::Folder(dir.to_path_buf())).map_err(|e| e.0)?;
        walk(&repo, n, Path::new(&n.name), &restored, &mut tally)?;
    }
    println!(
        "{name}: {} files, {} checked · {} identical · {} changed since the backup · {} deleted since · {} FAULTS",
        tally.files,
        human_bytes(tally.bytes),
        tally.same,
        tally.changed_since,
        tally.deleted_since,
        tally.faults.len()
    );
    for n in tally.notes.iter().take(20) {
        println!("  since: {n}");
    }
    if tally.notes.len() > 20 {
        println!("  since: …and {} more", tally.notes.len() - 20);
    }
    for f in &tally.faults {
        println!("  FAULT: {f}");
    }
    Ok(tally.faults.is_empty())
}

fn walk(repo: &keepr_engine::repo::Repo, n: &Node, src: &Path, restored: &Path, t: &mut Tally) -> Result<(), String> {
    match n.kind {
        NodeKind::Dir => {
            if let Some(sub) = n.subtree {
                for c in &repo.load_tree(&sub).map_err(|e| e.0)?.nodes {
                    walk(repo, c, &src.join(&c.name), &restored.join(&c.name), t)?;
                }
            }
        }
        NodeKind::Symlink => {
            let got = fs::read_link(restored).ok().map(|p| p.to_string_lossy().to_string());
            if got != n.target {
                t.faults.push(format!("{}: link restored as {got:?}, backed up as {:?}", src.display(), n.target));
            }
        }
        NodeKind::File => {
            t.files += 1;
            t.bytes += n.size;
            let rm = match fs::metadata(restored) {
                Ok(m) => m,
                Err(e) => {
                    t.faults.push(format!("{}: not restored ({e})", src.display()));
                    return Ok(());
                }
            };
            if rm.len() != n.size {
                t.faults.push(format!("{}: restored {} bytes, backed up {}", src.display(), rm.len(), n.size));
                return Ok(());
            }
            let sm = match fs::symlink_metadata(src) {
                Ok(m) => m,
                Err(_) => {
                    t.deleted_since += 1;
                    t.notes.push(format!("{} (deleted)", src.display()));
                    return Ok(());
                }
            };
            let as_backed_up = sm.len() == n.size && sm.mtime() * 1_000_000_000 + sm.mtime_nsec() == n.mtime;
            match same_bytes(src, restored) {
                Ok(true) => t.same += 1,
                Ok(false) if !as_backed_up => {
                    t.changed_since += 1;
                    t.notes.push(format!("{} (changed)", src.display()));
                }
                Ok(false) => t.faults.push(format!("{}: contents differ though size and date match", src.display())),
                Err(e) if !as_backed_up => {
                    t.changed_since += 1;
                    t.notes.push(format!("{} (changed; {e})", src.display()));
                }
                Err(e) => t.faults.push(format!("{}: couldn't compare ({e})", src.display())),
            }
        }
    }
    Ok(())
}

fn same_bytes(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (fa, fb) = (fs::File::open(a)?, fs::File::open(b)?);
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let (mut ra, mut rb) = (BufReader::with_capacity(1 << 20, fa), BufReader::with_capacity(1 << 20, fb));
    loop {
        let (ba, bb) = (ra.fill_buf()?, rb.fill_buf()?);
        if ba.is_empty() || bb.is_empty() {
            return Ok(ba.is_empty() && bb.is_empty());
        }
        let n = ba.len().min(bb.len());
        if ba[..n] != bb[..n] {
            return Ok(false);
        }
        ra.consume(n);
        rb.consume(n);
    }
}
