// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Forgetting old snapshots and reclaiming the space only they used.
//!
//! Safe to stop at any point: new packs are written first, then a new index naming them, and only
//! then are old index files and packs deleted. Until the last step the old state is still whole.

use crate::backup::Control;
use crate::repo::{pack_path, Entry, PackRecord, Repo, PACK_TARGET};
use crate::retention::{keep, parse_time, Retention};
use crate::tree::NodeKind;
use crate::{Id, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Default, Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Pruned {
    pub forgotten: u64,
    pub kept: u64,
    pub packs_deleted: u64,
    pub packs_rewritten: u64,
    /// Small packs copied together into fuller ones.
    pub packs_merged: u64,
    pub bytes_freed: u64,
}

/// A pack under this is small: typically the last of a backup, written however little it held.
const SMALL_PACK: u64 = PACK_TARGET as u64 / 4;
/// Small packs are merged once there are this many; for fewer, the rewrite isn't worth it.
const MERGE_AT: usize = 8;

/// Every blob the remaining snapshots need.
pub fn used_blobs(repo: &Repo, ctl: &Control) -> Result<HashSet<Id>> {
    let mut used = HashSet::new();
    let mut stack: Vec<Id> = repo.snapshots()?.iter().map(|s| s.tree).collect();
    while let Some(t) = stack.pop() {
        if !used.insert(t) {
            continue; // a tree already walked
        }
        ctl.checkpoint()?;
        let tree = repo.load_tree(&t)?;
        for n in &tree.nodes {
            match n.kind {
                NodeKind::Dir => stack.extend(n.subtree),
                NodeKind::File => used.extend(n.content.iter().copied()),
                NodeKind::Symlink => {}
            }
        }
    }
    Ok(used)
}

pub fn run(repo: &Arc<Repo>, r: &Retention, ctl: &Control) -> Result<Pruned> {
    let _lock = repo.lock(true)?;
    let mut out = Pruned::default();

    // Which snapshots go.
    let snaps = repo.snapshots()?;
    let times: Vec<_> = snaps.iter().map(|s| parse_time(&s.time).unwrap_or_else(chrono::Local::now)).collect();
    let mut kept = keep(r, &times, chrono::Local::now());
    if r.keep_deleted_days > 0 {
        keep_deleted(repo, &snaps, &times, &mut kept, r.keep_deleted_days, ctl)?;
    }
    for (i, s) in snaps.iter().enumerate() {
        if !kept.contains(&i) {
            repo.forget(&s.id)?;
            out.forgotten += 1;
        }
    }
    out.kept = kept.len() as u64;
    reclaim(repo, ctl, &mut out)?;
    Ok(out)
}

/// Files in `tree` that no kept snapshot has, at the same path with the same contents.
fn uncovered(repo: &Repo, tree: Id, others: &[Id], base: &str, out: &mut Vec<String>) -> Result<()> {
    if others.contains(&tree) || out.len() >= 500 {
        return Ok(());
    }
    let t = repo.load_tree(&tree)?;
    let theirs: Vec<Arc<crate::tree::Tree>> = others.iter().map(|o| repo.load_tree(o)).collect::<Result<_>>()?;
    for n in &t.nodes {
        let path = if base.is_empty() { n.name.clone() } else { format!("{base}/{}", n.name) };
        let same: Vec<&crate::tree::Node> = theirs.iter().filter_map(|t| t.get(&n.name)).collect();
        match n.kind {
            NodeKind::File => {
                if !same.iter().any(|o| o.content_key() == n.content_key()) {
                    out.push(path);
                }
            }
            NodeKind::Dir => {
                if let Some(sub) = n.subtree {
                    let subs: Vec<Id> = same.iter().filter_map(|o| o.subtree).collect();
                    uncovered(repo, sub, &subs, &path, out)?;
                }
            }
            NodeKind::Symlink => {}
        }
    }
    Ok(())
}

/// Keeps, beyond the retention rules, any snapshot holding the last copy of a file that was
/// deleted less than `days` ago.
fn keep_deleted(
    repo: &Repo,
    snaps: &[crate::repo::Snapshot],
    times: &[chrono::DateTime<chrono::Local>],
    kept: &mut HashSet<usize>,
    days: u32,
    ctl: &Control,
) -> Result<()> {
    let now = chrono::Local::now();
    // Newest first, so a snapshot kept here covers older ones holding the same file.
    for i in (0..snaps.len()).rev() {
        if kept.contains(&i) {
            continue;
        }
        ctl.checkpoint()?;
        let others: Vec<Id> = kept.iter().map(|&k| snaps[k].tree).collect();
        let mut files = Vec::new();
        uncovered(repo, snaps[i].tree, &others, "", &mut files)?;
        let recent_deletion = files.iter().any(|f| {
            // When it went: the first later snapshot without it.
            snaps
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, s)| crate::browse::node_at(repo, s, f).ok().flatten().is_none())
                .is_some_and(|(j, _)| now.signed_duration_since(times[j]).num_days() < days as i64)
        });
        if recent_deletion {
            kept.insert(i);
        }
    }
    Ok(())
}

/// Total size and file count of a tree, using folders' own totals where they have them.
fn size_of(repo: &Repo, t: &crate::tree::Tree) -> Result<(u64, u64)> {
    let (mut bytes, mut files) = (0, 0);
    for n in &t.nodes {
        match n.kind {
            NodeKind::File => {
                bytes += n.size;
                files += 1;
            }
            NodeKind::Dir if n.files > 0 || n.size > 0 => {
                bytes += n.size;
                files += n.files;
            }
            NodeKind::Dir => {
                if let Some(sub) = n.subtree {
                    let (b, f) = crate::browse::du(repo, &sub)?;
                    bytes += b;
                    files += f;
                }
            }
            NodeKind::Symlink => {}
        }
    }
    Ok((bytes, files))
}

/// A tree without the entry at `names` (a path below it). Returns the new tree's id and the size
/// and file count taken out, or None if the path isn't there.
fn without(repo: &Repo, tree: &Id, names: &[&str]) -> Result<Option<(Id, u64, u64)>> {
    let mut t = (*repo.load_tree(tree)?).clone();
    let Some(i) = t.nodes.iter().position(|n| n.name == names[0]) else { return Ok(None) };
    let (bytes, files) = if names.len() == 1 {
        let n = t.nodes.remove(i);
        match n.kind {
            NodeKind::File => (n.size, 1),
            NodeKind::Dir => {
                if n.size > 0 || n.files > 0 {
                    (n.size, n.files)
                } else {
                    n.subtree.map(|s| crate::browse::du(repo, &s)).transpose()?.unwrap_or((0, 0))
                }
            }
            NodeKind::Symlink => (0, 0),
        }
    } else {
        let Some(sub) = t.nodes[i].subtree else { return Ok(None) };
        let Some((new, b, f)) = without(repo, &sub, &names[1..])? else { return Ok(None) };
        let n = &mut t.nodes[i];
        n.subtree = Some(new);
        n.size = n.size.saturating_sub(b);
        n.files = n.files.saturating_sub(f);
        (b, f)
    };
    Ok(Some((repo.save_tree(&t)?, bytes, files)))
}

/// Removes a file or folder (by its original path) from every snapshot, then frees the space
/// only it used. A source's own path removes the whole source. As with sources, new snapshots
/// are written before the old ones are removed.
pub fn remove_path(repo: &Arc<Repo>, path: &str, ctl: &Control) -> Result<Pruned> {
    let path = path.trim_end_matches('/');
    let is_source = repo.snapshots()?.iter().any(|s| s.sources.iter().any(|x| x.trim_end_matches('/') == path));
    if is_source {
        return remove_source(repo, path, ctl);
    }
    let _lock = repo.lock(true)?;
    let mut out = Pruned::default();
    for s in repo.snapshots()? {
        ctl.checkpoint()?;
        let Some(src) =
            s.sources.iter().filter(|x| path.starts_with(&format!("{}/", x.trim_end_matches('/')))).max_by_key(|x| x.len()).cloned()
        else {
            out.kept += 1;
            continue;
        };
        let rest: Vec<&str> = path[src.trim_end_matches('/').len()..].trim_start_matches('/').split('/').collect();
        let mut names = vec![src.as_str()];
        names.extend(rest);
        let Some((tree, b, f)) = without(repo, &s.tree, &names)? else {
            out.kept += 1;
            continue;
        };
        let mut ns = s.clone();
        ns.tree = tree;
        ns.stats.bytes = ns.stats.bytes.saturating_sub(b);
        ns.stats.files = ns.stats.files.saturating_sub(f);
        repo.flush()?;
        repo.save_snapshot(&mut ns)?;
        repo.forget(&s.id)?;
        out.forgotten += 1;
    }
    reclaim(repo, ctl, &mut out)?;
    Ok(out)
}

/// Removes a source (by its path, as the snapshots name it) from every snapshot, then frees the
/// space only it used. New snapshots are written before the old ones are removed, so a stop
/// part-way leaves both, never neither.
pub fn remove_source(repo: &Arc<Repo>, source: &str, ctl: &Control) -> Result<Pruned> {
    let _lock = repo.lock(true)?;
    let mut out = Pruned::default();
    let key = source.trim_end_matches('/');
    for s in repo.snapshots()? {
        ctl.checkpoint()?;
        let root = repo.load_tree(&s.tree)?;
        if root.get(key).is_none() && !s.sources.iter().any(|x| x.trim_end_matches('/') == key) {
            out.kept += 1;
            continue;
        }
        let mut t = (*root).clone();
        t.nodes.retain(|n| n.name.trim_end_matches('/') != key);
        let mut ns = s.clone();
        ns.tree = repo.save_tree(&t)?;
        ns.sources.retain(|x| x.trim_end_matches('/') != key);
        // Its size figures, for what's left.
        let (bytes, files) = size_of(repo, &t)?;
        ns.stats.bytes = bytes;
        ns.stats.files = files;
        repo.flush()?;
        let old = s.id;
        repo.save_snapshot(&mut ns)?;
        repo.forget(&old)?;
        out.forgotten += 1;
    }
    reclaim(repo, ctl, &mut out)?;
    Ok(out)
}

/// A pack's blobs still in use, with where each sits in it.
type Live = Vec<(Id, crate::repo::Loc)>;

/// Rewrites packs that are mostly unused, merges small ones, deletes ones that are wholly unused,
/// and replaces the index with one naming only what is left.
pub fn reclaim(repo: &Arc<Repo>, ctl: &Control, out: &mut Pruned) -> Result<()> {
    let used = used_blobs(repo, ctl)?;
    let (packs, blobs, old_files) = {
        let ix = repo.index.read().unwrap();
        (ix.packs.clone(), ix.blobs.clone(), ix.files.clone())
    };
    let mut live: HashMap<u32, Vec<(Id, crate::repo::Loc)>> = HashMap::new();
    for (id, loc) in &blobs {
        if used.contains(id) {
            live.entry(loc.pack).or_default().push((*id, *loc));
        }
    }
    let mut records: Vec<PackRecord> = Vec::new();
    let mut doomed: Vec<Id> = Vec::new();
    let mut rewrite: Vec<(Id, Live)> = Vec::new();
    // Every backup ends with a pack of whatever was left over, often a few KB. A cloud folder
    // syncs each as its own file, so once enough pile up they are copied together.
    let mut small: Vec<(Id, Live, u64)> = Vec::new();
    for (n, pack) in packs.iter().enumerate() {
        let size = repo.backend.size(&pack_path(pack)).unwrap_or(0);
        let entries = live.remove(&(n as u32)).unwrap_or_default();
        let live_bytes: u64 = entries.iter().map(|(_, l)| l.len as u64).sum();
        if entries.is_empty() {
            doomed.push(*pack);
            out.bytes_freed += size;
        } else if live_bytes * 2 < size {
            out.bytes_freed += size - live_bytes;
            rewrite.push((*pack, entries));
        } else if size < SMALL_PACK {
            small.push((*pack, entries, size - live_bytes));
        } else {
            let mut blobs: Vec<Entry> = entries.iter().map(|(id, l)| Entry(*id, l.kind, l.offset, l.len, l.raw)).collect();
            blobs.sort_by_key(|e| e.2);
            records.push(PackRecord { pack: *pack, blobs });
        }
    }
    if small.len() >= MERGE_AT {
        out.packs_merged = small.len() as u64;
        for (pack, entries, overhead) in small {
            out.bytes_freed += overhead;
            rewrite.push((pack, entries));
        }
    } else {
        for (pack, entries, _) in small {
            let mut blobs: Vec<Entry> = entries.iter().map(|(id, l)| Entry(*id, l.kind, l.offset, l.len, l.raw)).collect();
            blobs.sort_by_key(|e| e.2);
            records.push(PackRecord { pack, blobs });
        }
    }

    // Copy the live blobs of thin packs into new ones, still sealed: no need to decrypt.
    let mut buf: Vec<u8> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let write = |buf: &mut Vec<u8>, entries: &mut Vec<Entry>, records: &mut Vec<PackRecord>| -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let header = repo.encode(&serde_json::to_vec(&entries)?);
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
        let id = Id::random();
        repo.backend.write(&pack_path(&id), buf)?;
        records.push(PackRecord { pack: id, blobs: std::mem::take(entries) });
        buf.clear();
        Ok(())
    };
    for (pack, blobs) in &rewrite {
        ctl.checkpoint()?;
        for (id, loc) in blobs {
            let stored = repo.load_stored(pack, loc)?;
            entries.push(Entry(*id, loc.kind, buf.len() as u32, stored.len() as u32, loc.raw));
            buf.extend_from_slice(&stored);
            if buf.len() >= PACK_TARGET {
                write(&mut buf, &mut entries, &mut records)?;
            }
        }
        doomed.push(*pack);
    }
    write(&mut buf, &mut entries, &mut records)?;

    // Packs on disk that no index names: left by a backup that stopped before its index.
    let known: HashSet<String> = packs.iter().chain(records.iter().map(|r| &r.pack)).map(pack_path).collect();
    for f in repo.backend.list("packs")? {
        let p = format!("packs/{f}");
        if f.ends_with(".pack") && !known.contains(&p) {
            out.bytes_freed += repo.backend.size(&p).unwrap_or(0);
            repo.backend.remove(&p)?;
            out.packs_deleted += 1;
        }
    }

    if doomed.is_empty() && out.forgotten == 0 && old_files.len() <= 1 {
        return Ok(()); // nothing to tidy
    }
    let path = format!("index/{}.idx", Id::random().hex());
    let raw = serde_json::to_vec(&serde_json::json!({ "packs": records }))?;
    repo.backend.write(&path, &repo.encode(&raw))?;
    for f in &old_files {
        repo.backend.remove(f)?;
    }
    for p in &doomed {
        repo.backend.remove(&pack_path(p))?;
        out.packs_deleted += 1;
    }
    out.packs_rewritten = rewrite.len() as u64 - out.packs_merged;
    out.packs_deleted -= rewrite.len() as u64;
    reload_index(repo)
}

/// Deletes packs no index names: what a backup leaves when it's stopped hard (Keepr quit, the
/// Mac slept for good, the app crashed) before it wrote its index. Returns how many packs and
/// bytes. Only when nobody else is using the repository: another Mac's backup still running
/// has packs no index names *yet*. The index is read afresh first for the same reason.
pub fn remove_leftovers(repo: &Arc<Repo>) -> Result<(u64, u64)> {
    let Ok(_lock) = repo.lock(true) else { return Ok((0, 0)) };
    reload_index(repo)?;
    let known: HashSet<String> = repo.index.read().unwrap().packs.iter().map(pack_path).collect();
    let (mut n, mut bytes) = (0, 0);
    for f in repo.backend.list("packs")? {
        let p = format!("packs/{f}");
        if f.ends_with(".pack") && !known.contains(&p) {
            bytes += repo.backend.size(&p).unwrap_or(0);
            repo.backend.remove(&p)?;
            n += 1;
        }
    }
    Ok((n, bytes))
}

fn reload_index(repo: &Repo) -> Result<()> {
    let mut fresh = crate::repo::Index::default();
    for f in repo.backend.list("index")? {
        if !f.ends_with(".idx") {
            continue;
        }
        let path = format!("index/{f}");
        let plain = repo.decode(&repo.backend.read(&path)?, None)?;
        let doc: serde_json::Value = serde_json::from_slice(&plain)?;
        let recs: Vec<PackRecord> = serde_json::from_value(doc["packs"].clone())?;
        for r in &recs {
            fresh.add_record(r);
        }
        fresh.files.push(path);
    }
    *repo.index.write().unwrap() = fresh;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{self, tests::*};
    use std::fs;

    #[test]
    fn small_packs_are_merged_once_there_are_enough() {
        let (src, dst, repo) = setup(None);
        let keep_all = Retention { all_hours: 1000, daily_days: 0, weekly_weeks: 0, monthly_months: 0, keep_deleted_days: 0 };
        let packs = |r: &Repo| r.backend.list("packs").unwrap().iter().filter(|f| f.ends_with(".pack")).count();
        let mut parent = None;
        for i in 0..MERGE_AT + 1 {
            fs::write(src.path().join(format!("{i}.txt")), format!("file {i}")).unwrap();
            parent = Some(backup::run(&repo, &opts(src.path()), parent.as_ref(), &Control::default()).unwrap());
            if i == MERGE_AT - 2 {
                // Too few to be worth it yet.
                let p = run(&repo, &keep_all, &Control::default()).unwrap();
                assert_eq!(p.packs_merged, 0, "{p:?}");
            }
        }
        let before = packs(&repo);
        assert!(before > MERGE_AT, "{before}");
        let p = run(&repo, &keep_all, &Control::default()).unwrap();
        assert_eq!((p.forgotten, p.packs_merged as usize, p.packs_rewritten), (0, before, 0), "{p:?}");
        assert_eq!(packs(&repo), 1);
        let r2 = Arc::new(crate::repo::Repo::open(Arc::new(crate::backend::Folder::new(dst.path())), crate::repo::Secret::None).unwrap());
        assert_eq!(r2.snapshots().unwrap().len(), MERGE_AT + 1);
        assert!(crate::check::run(&r2, 1.0, &Control::default()).unwrap().problems.is_empty());
    }

    #[test]
    fn leftovers_from_a_stopped_backup_go() {
        let (src, _d, repo) = crate::backup::tests::setup(None);
        std::fs::write(src.path().join("a.txt"), b"alpha").unwrap();
        let s1 = crate::backup::run(&repo, &crate::backup::tests::opts(src.path()), None, &Control::default()).unwrap();
        repo.backend.write("packs/ff/ff00.pack", &[0u8; 1000]).unwrap();
        assert_eq!(remove_leftovers(&repo).unwrap(), (1, 1000));
        assert!(!repo.backend.exists("packs/ff/ff00.pack"));
        assert_eq!(remove_leftovers(&repo).unwrap(), (0, 0));
        let _ = s1;
        assert!(crate::check::run(&repo, 1.0, &Control::default()).unwrap().problems.is_empty());
    }

    #[test]
    fn removes_a_source_from_every_snapshot() {
        let (a, dst, repo) = setup(None);
        let b = tempfile::tempdir().unwrap();
        fs::write(a.path().join("keep.txt"), b"keep").unwrap();
        let big: Vec<u8> = {
            let mut x = 7u64;
            (0..2_000_000)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect()
        };
        fs::write(b.path().join("gone.bin"), &big).unwrap();
        let mut o = opts(a.path());
        o.sources.push(b.path().to_path_buf());
        let s1 = backup::run(&repo, &o, None, &Control::default()).unwrap();
        backup::run(&repo, &o, Some(&s1), &Control::default()).unwrap();
        let before: u64 = repo.backend.list("packs").unwrap().iter().map(|f| repo.backend.size(&format!("packs/{f}")).unwrap()).sum();
        let p = remove_source(&repo, &b.path().to_string_lossy(), &Control::default()).unwrap();
        assert_eq!(p.forgotten, 2, "both snapshots rewritten");
        let after: u64 = repo.backend.list("packs").unwrap().iter().map(|f| repo.backend.size(&format!("packs/{f}")).unwrap()).sum();
        assert!(before - after > 1_900_000, "{before} -> {after}");
        let r2 = Arc::new(crate::repo::Repo::open(Arc::new(crate::backend::Folder::new(dst.path())), crate::repo::Secret::None).unwrap());
        let snaps = r2.snapshots().unwrap();
        assert_eq!(snaps.len(), 2);
        for s in &snaps {
            assert_eq!(s.sources, vec![a.path().to_string_lossy().to_string()]);
            assert_eq!((s.stats.bytes, s.stats.files), (4, 1), "sizes are for what's left");
            assert!(crate::browse::node_at(&r2, s, &format!("{}/keep.txt", a.path().display())).unwrap().is_some());
        }
        assert!(crate::check::run(&r2, 1.0, &Control::default()).unwrap().problems.is_empty());
    }

    #[test]
    fn removes_a_folder_inside_a_source_everywhere() {
        let (a, _dst, repo) = setup(Some("pw"));
        fs::create_dir_all(a.path().join("Pictures/Photos Library.photoslibrary/originals")).unwrap();
        let big: Vec<u8> = {
            let mut x = 9u64;
            (0..2_000_000)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect()
        };
        fs::write(a.path().join("Pictures/Photos Library.photoslibrary/originals/IMG.heic"), &big).unwrap();
        fs::write(a.path().join("Pictures/keep.jpg"), b"keep").unwrap();
        let s1 = backup::run(&repo, &opts(a.path()), None, &Control::default()).unwrap();
        backup::run(&repo, &opts(a.path()), Some(&s1), &Control::default()).unwrap();
        let lib = format!("{}/Pictures/Photos Library.photoslibrary", a.path().display());
        let p = remove_path(&repo, &lib, &Control::default()).unwrap();
        assert_eq!(p.forgotten, 2);
        assert!(p.bytes_freed > 1_900_000, "{p:?}");
        for s in repo.snapshots().unwrap() {
            assert!(crate::browse::node_at(&repo, &s, &lib).unwrap().is_none());
            assert!(crate::browse::node_at(&repo, &s, &format!("{}/Pictures/keep.jpg", a.path().display())).unwrap().is_some());
            assert_eq!((s.stats.bytes, s.stats.files), (4, 1));
            let top = repo.load_tree(&s.tree).unwrap();
            assert_eq!(top.nodes[0].size, 4, "folder totals updated up the chain");
        }
        assert!(crate::check::run(&repo, 1.0, &Control::default()).unwrap().problems.is_empty());
    }

    #[test]
    fn forgets_and_frees_space_but_keeps_what_is_needed() {
        let (src, dst, repo) = setup(Some("pw"));
        // Incompressible, so the space freed is the data's own size.
        let noise = |seed: u64| {
            let mut x = seed;
            (0..3_000_000)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect::<Vec<u8>>()
        };
        fs::write(src.path().join("keep.bin"), noise(1)).unwrap();
        fs::write(src.path().join("gone.bin"), noise(2)).unwrap();
        let s1 = backup::run(&repo, &opts(src.path()), None, &Control::default()).unwrap();
        fs::remove_file(src.path().join("gone.bin")).unwrap();
        let _s2 = backup::run(&repo, &opts(src.path()), Some(&s1), &Control::default()).unwrap();
        // Forget the first by making the policy keep only the newest.
        // Keeping deleted files for 90 days keeps the first snapshot, which has gone.bin.
        let mut r = Retention { all_hours: 0, daily_days: 0, weekly_weeks: 0, monthly_months: 1, keep_deleted_days: 90 };
        let p = run(&repo, &r, &Control::default()).unwrap();
        assert_eq!(p.forgotten, 0);
        r.keep_deleted_days = 0;
        let p = run(&repo, &r, &Control::default()).unwrap();
        assert_eq!(p.forgotten, 1);
        assert!(p.bytes_freed > 2_000_000, "{p:?}");
        // Everything the remaining snapshot needs still reads back, also after reopening.
        let r2 = Arc::new(
            crate::repo::Repo::open(Arc::new(crate::backend::Folder::new(dst.path())), crate::repo::Secret::Password("pw")).unwrap(),
        );
        let rep = crate::check::run(&r2, 1.0, &Control::default()).unwrap();
        assert!(rep.problems.is_empty(), "{:?}", rep.problems);
        assert_eq!(r2.snapshots().unwrap().len(), 1);
    }
}
