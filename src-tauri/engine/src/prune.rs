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
    pub bytes_freed: u64,
}

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
    let kept = keep(r, &times, chrono::Local::now());
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

/// Rewrites packs that are mostly unused, deletes ones that are wholly unused, and replaces
/// the index with one naming only what is left.
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
    let mut rewrite: Vec<(Id, Vec<(Id, crate::repo::Loc)>)> = Vec::new();
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
        } else {
            let mut blobs: Vec<Entry> = entries.iter().map(|(id, l)| Entry(*id, l.kind, l.offset, l.len, l.raw)).collect();
            blobs.sort_by_key(|e| e.2);
            records.push(PackRecord { pack: *pack, blobs });
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
        out.packs_rewritten += 1;
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
    out.packs_deleted -= out.packs_rewritten;
    reload_index(repo)

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
    fn forgets_and_frees_space_but_keeps_what_is_needed() {
        let (src, dst, repo) = setup(Some("pw"));
        // Incompressible, so the space freed is the data's own size.
        let noise = |seed: u64| {
            let mut x = seed;
            (0..3_000_000).map(|_| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x as u8 }).collect::<Vec<u8>>()
        };
        fs::write(src.path().join("keep.bin"), noise(1)).unwrap();
        fs::write(src.path().join("gone.bin"), noise(2)).unwrap();
        let s1 = backup::run(&repo, &opts(src.path()), None, &Control::default()).unwrap();
        fs::remove_file(src.path().join("gone.bin")).unwrap();
        let _s2 = backup::run(&repo, &opts(src.path()), Some(&s1), &Control::default()).unwrap();
        // Forget the first by making the policy keep only the newest.
        let r = Retention { all_hours: 0, daily_days: 0, weekly_weeks: 0, monthly_months: 1 };
        let p = run(&repo, &r, &Control::default()).unwrap();
        assert_eq!(p.forgotten, 1);
        assert!(p.bytes_freed > 2_000_000, "{p:?}");
        // Everything the remaining snapshot needs still reads back, also after reopening.
        let r2 = Arc::new(crate::repo::Repo::open(Arc::new(crate::backend::Folder::new(dst.path())), crate::repo::Secret::Password("pw")).unwrap());
        let rep = crate::check::run(&r2, 1.0, &Control::default()).unwrap();
        assert!(rep.problems.is_empty(), "{:?}", rep.problems);
        assert_eq!(r2.snapshots().unwrap().len(), 1);
    }
}
