// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stored map: a plan's space in its destination, after de-duplication and compression,
//! shared out among the folders it came from, so it adds up to the Space used card's figure (the
//! sum of every blob in the index). Each piece of data counts once, in the folder of the file that
//! first stored it (the oldest snapshot still kept that has it), so a folder since deleted still
//! shows what its versions hold. The folder listings are a block of their own, as is data no
//! snapshot uses any more, which a tidy-up frees. A folder's growth is what the snapshots since the
//! week before (browse::week_before) stored there.

use crate::browse::{week_before, MapDir, MAP_BUDGET, MAP_KIDS};
use crate::core::Core;
use keepr_engine::repo::{Kind, Repo};
use keepr_engine::tree::NodeKind;
use keepr_engine::Id;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// One folder: what was first stored by the files directly in it, and its folders.
#[derive(Default)]
struct Folder {
    own: u64,
    own_week: u64,
    /// File versions that stored something here.
    own_files: u64,
    kids: BTreeMap<String, Folder>,
    // Filled in by `sum`: the folder with everything below it.
    size: u64,
    week: u64,
    files: u64,
}

impl Folder {
    fn sum(&mut self) {
        let (mut s, mut w, mut f) = (self.own, self.own_week, self.own_files);
        for k in self.kids.values_mut() {
            k.sum();
            s += k.size;
            w += k.week;
            f += k.files;
        }
        (self.size, self.week, self.files) = (s, w, f);
    }
}

/// A plan's shared-out space, worked out once for a set of snapshots and an index.
struct Shares {
    top: Folder,
    listings: (u64, u64),
    unused: u64,
    since: Option<String>,
    weekly: bool,
}

/// Kept per plan until its snapshots or its index change: working it out reads every listing.
/// By plan: what it was worked out from (its snapshots and index size), and the result.
type Cache = HashMap<String, (String, Arc<Shares>)>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn shares(repo: &Repo) -> Result<Shares, String> {
    let snaps = repo.snapshots().map_err(|e| e.0)?;
    let blobs: HashMap<Id, (u64, Kind)> = repo.index.read().unwrap().blobs.iter().map(|(id, l)| (*id, (l.len as u64, l.kind))).collect();
    let then = snaps.last().and_then(|l| week_before(&snaps, l));
    let latest = snaps.last().and_then(|s| chrono::DateTime::parse_from_rfc3339(&s.time).ok());
    let at = then.and_then(|t| chrono::DateTime::parse_from_rfc3339(&t.time).ok());
    let mut seen: HashSet<Id> = HashSet::new();
    let mut top = Folder::default();
    let mut listings = (0u64, 0u64);
    // Oldest first, so data counts where it was first stored.
    for s in &snaps {
        let this_week = match (at, chrono::DateTime::parse_from_rfc3339(&s.time).ok()) {
            (Some(a), Some(t)) => t > a,
            _ => false,
        };
        let mut todo: Vec<(Id, Vec<String>)> = vec![(s.tree, vec![])];
        while let Some((tid, path)) = todo.pop() {
            // A listing already counted holds nothing new: everything below it was counted with it.
            if !seen.insert(tid) {
                continue;
            }
            let len = blobs.get(&tid).map_or(0, |b| b.0);
            listings.0 += len;
            if this_week {
                listings.1 += len;
            }
            let t = repo.load_tree(&tid).map_err(|e| e.0)?;
            for n in &t.nodes {
                match n.kind {
                    NodeKind::Dir => {
                        if let Some(sub) = n.subtree {
                            let mut p = path.clone();
                            p.push(n.name.clone());
                            todo.push((sub, p));
                        }
                    }
                    _ => {
                        let mut new = 0;
                        for c in &n.content {
                            if seen.insert(*c) {
                                new += blobs.get(c).map_or(0, |b| b.0);
                            }
                        }
                        if new > 0 {
                            let mut f = &mut top;
                            for seg in &path {
                                f = f.kids.entry(seg.clone()).or_default();
                            }
                            f.own += new;
                            f.own_files += 1;
                            if this_week {
                                f.own_week += new;
                            }
                        }
                    }
                }
            }
        }
    }
    top.sum();
    let unused = blobs.iter().filter(|(id, _)| !seen.contains(id)).map(|(_, b)| b.0).sum();
    let weekly = match (at, latest) {
        (Some(a), Some(b)) => (6..=9).contains(&(b - a).num_days()),
        _ => false,
    };
    Ok(Shares { top, listings, unused, since: then.map(|t| t.time.clone()), weekly })
}

fn cached(core: &Core, plan: &str) -> Result<Arc<Shares>, String> {
    let repo = core.repo(plan, false)?;
    let snaps = repo.snapshots().map_err(|e| e.0)?;
    let key = format!("{}:{}", snaps.iter().map(|s| s.id.hex()).collect::<Vec<_>>().join(","), repo.index.read().unwrap().blobs.len());
    if let Some((k, s)) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(plan) {
        if *k == key {
            return Ok(s.clone());
        }
    }
    let s = Arc::new(shares(&repo)?);
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(plan.to_string(), (key, s.clone()));
    Ok(s)
}

/// A folder by its path, "" for the top. The top's folders are the sources, named by their whole
/// path ("/Users/me/Documents").
fn find<'a>(sh: &'a Shares, path: &str) -> Option<&'a Folder> {
    if path.is_empty() {
        return Some(&sh.top);
    }
    let (src, rest) =
        sh.top.kids.keys().find_map(|k| (path == k).then_some((k, "")).or_else(|| path.strip_prefix(&format!("{k}/")).map(|r| (k, r))))?;
    let mut here = &sh.top.kids[src];
    for seg in rest.split('/').filter(|s| !s.is_empty()) {
        here = here.kids.get(seg)?;
    }
    Some(here)
}

/// What a folder holds stored, and what this week's backups stored in it, for Ask (ask.rs).
pub fn stored_of(core: &Core, plan: &str, path: &str) -> Option<(u64, u64)> {
    let sh = cached(core, plan).ok()?;
    find(&sh, path).map(|f| (f.size, f.week))
}

/// The stored map of `path` ("" for the plan's top), as the size map draws it.
pub fn stored_map(core: &Core, plan: &str, path: &str) -> Result<MapDir, String> {
    let sh = cached(core, plan)?;
    let here = find(&sh, path).ok_or("Nothing from that folder is stored.")?;
    let compare = sh.since.is_some();
    let grew = |w: u64| compare.then_some(w as i64);
    let name = if path.is_empty() { String::new() } else { path.rsplit('/').next().unwrap_or(path).to_string() };
    let mut size = here.size;
    let mut week = here.week;
    if path.is_empty() {
        size += sh.listings.0 + sh.unused;
        week += sh.listings.1;
    }
    let root = MapDir {
        name,
        path: path.to_string(),
        size,
        files: here.files,
        depth: 0,
        grew: grew(week),
        since: sh.since.clone(),
        weekly: sh.weekly,
        ..Default::default()
    };
    // As browse::size_map: a flat list filled biggest first, within the same budget.
    let mut all: Vec<(usize, MapDir, Option<&Folder>)> = vec![(usize::MAX, root, Some(here))];
    let mut todo = std::collections::BinaryHeap::from([(here.size, 0usize)]);
    let floor = size / 50_000;
    while let Some((_, i)) = todo.pop() {
        if all.len() >= MAP_BUDGET {
            break;
        }
        let Some(f) = all[i].2 else { continue };
        let d = &mut all[i].1;
        d.read = true;
        d.loose_files = f.own_files;
        d.loose_size = f.own;
        let mut dirs: Vec<(&String, &Folder)> = f.kids.iter().filter(|(_, k)| k.size > 0).collect();
        dirs.sort_by_key(|(_, k)| std::cmp::Reverse(k.size));
        let (depth, base) = (d.depth + 1, d.path.clone());
        for (k, (nm, sub)) in dirs.into_iter().enumerate() {
            if k >= MAP_KIDS || sub.size < floor || all.len() >= MAP_BUDGET {
                let d = &mut all[i].1;
                d.more += 1;
                d.more_size += sub.size;
                continue;
            }
            let p = if base.is_empty() { nm.clone() } else { format!("{base}/{nm}") };
            all.push((
                i,
                MapDir { name: nm.clone(), path: p, size: sub.size, files: sub.files, depth, grew: grew(sub.week), ..Default::default() },
                Some(sub),
            ));
            todo.push((sub.size, all.len() - 1));
        }
    }
    if path.is_empty() {
        let special = |kind: &str, name: &str, size: u64, week: Option<i64>| MapDir {
            name: name.into(),
            path: format!("\u{1}{kind}"),
            size,
            depth: 1,
            read: true,
            grew: week,
            special: Some(kind.into()),
            ..Default::default()
        };
        if sh.listings.0 > 0 {
            all.push((0, special("listings", "Folder listings", sh.listings.0, grew(sh.listings.1)), None));
        }
        if sh.unused > 0 {
            all.push((0, special("unused", "Not used by any snapshot", sh.unused, None), None));
        }
    }
    for j in (1..all.len()).rev() {
        let (parent, d, _) = std::mem::take(&mut all[j]);
        all[parent].1.kids.push(d);
    }
    let mut root = std::mem::take(&mut all[0].1);
    fn order(d: &mut MapDir) {
        d.kids.sort_by_key(|k| std::cmp::Reverse(k.size));
        d.kids.iter_mut().for_each(order);
    }
    order(&mut root);
    Ok(root)
}
