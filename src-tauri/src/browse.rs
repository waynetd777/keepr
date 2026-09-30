// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the Restore screen shows: a folder's entries in a snapshot, each marked new, changed or
//! deleted since the snapshot before, with how many versions it has had.

use crate::core::Core;
use keepr_engine::repo::{Repo, Snapshot};
use keepr_engine::tree::{Node, NodeKind};
use keepr_engine::Id;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// "file", "dir" or "link".
    pub kind: String,
    pub size: u64,
    /// Milliseconds since 1970.
    pub mtime: i64,
    /// "new", "changed", "deleted" or "".
    pub tag: String,
    pub versions: u32,
    /// For a folder, how many entries it has.
    pub items: u32,
    /// In the newest snapshot, against the Mac now: "gone" (in the backup, not on the Mac),
    /// "unsaved" (on the Mac, not in the snapshot) or "".
    pub disk: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SnapInfo {
    pub id: String,
    pub time: String,
    pub kind: String,
    pub files: u64,
    pub bytes: u64,
    pub changed: u64,
    pub added_bytes: u64,
}

pub fn info(s: &Snapshot) -> SnapInfo {
    SnapInfo {
        id: s.id.hex(),
        time: s.time.clone(),
        kind: s.kind.clone(),
        files: s.stats.files,
        bytes: s.stats.bytes,
        changed: s.stats.new_files + s.stats.changed_files,
        added_bytes: s.stats.added_bytes,
    }
}

fn kind(k: NodeKind) -> &'static str {
    match k {
        NodeKind::File => "file",
        NodeKind::Dir => "dir",
        NodeKind::Symlink => "link",
    }
}

/// A folder's tree id in each snapshot (None where the folder isn't).
fn dir_trees(repo: &Repo, snaps: &[Snapshot], path: &str) -> Vec<Option<Id>> {
    snaps
        .iter()
        .map(|s| {
            if path.is_empty() {
                Some(s.tree)
            } else {
                keepr_engine::browse::node_at(repo, s, path).ok().flatten().and_then(|n| n.subtree)
            }
        })
        .collect()
}

pub fn list(core: &Core, plan: &str, snapshot: &str, path: &str, show_deleted: bool) -> Result<Vec<Entry>, String> {
    let repo = core.repo(plan, false)?;
    let snaps = repo.snapshots().map_err(|e| e.0)?;
    let at = snaps.iter().position(|s| s.id.hex() == snapshot).ok_or("That snapshot is no longer in the backup.")?;
    let trees = dir_trees(&repo, &snaps, path);

    // Versions: walk the snapshots up to this one, counting changes of each name's contents.
    // Identical folders share a tree, so each distinct tree is read once.
    let mut loaded: HashMap<Id, std::sync::Arc<keepr_engine::tree::Tree>> = HashMap::new();
    let mut last: HashMap<String, String> = HashMap::new();
    let mut versions: HashMap<String, u32> = HashMap::new();
    for t in trees.iter().take(at + 1) {
        let Some(t) = t else {
            last.clear();
            continue;
        };
        if !loaded.contains_key(t) {
            loaded.insert(*t, repo.load_tree(t).map_err(|e| e.0)?);
        }
        let tree = &loaded[t];
        let mut seen = HashMap::new();
        for n in &tree.nodes {
            let key = n.content_key();
            if last.get(&n.name) != Some(&key) {
                *versions.entry(n.name.clone()).or_default() += 1;
            }
            seen.insert(n.name.clone(), key);
        }
        last = seen;
    }

    let here: Vec<Node> = match trees[at] {
        Some(t) => repo.load_tree(&t).map_err(|e| e.0)?.nodes.clone(),
        None => vec![],
    };
    let before: Option<Vec<Node>> =
        if at > 0 { trees[at - 1].map(|t| repo.load_tree(&t).map(|t| t.nodes.clone())).transpose().map_err(|e| e.0)? } else { None };
    let prev: HashMap<&str, &Node> = before.iter().flatten().map(|n| (n.name.as_str(), n)).collect();

    let join = |name: &str| if path.is_empty() { name.to_string() } else { format!("{path}/{name}") };
    let items = |n: &Node| -> u32 { n.subtree.and_then(|t| repo.load_tree(&t).ok()).map_or(0, |t| t.nodes.len() as u32) };
    let mut out: Vec<Entry> = here
        .iter()
        .map(|n| {
            let tag = match prev.get(n.name.as_str()) {
                None if before.is_some() => "new",
                Some(p) if p.content_key() != n.content_key() && n.kind == NodeKind::File => "changed",
                _ => "",
            };
            Entry {
                name: n.name.clone(),
                path: join(&n.name),
                kind: kind(n.kind).into(),
                size: n.size,
                mtime: n.mtime / 1_000_000,
                tag: tag.into(),
                versions: versions.get(&n.name).copied().unwrap_or(1),
                items: if n.kind == NodeKind::Dir { items(n) } else { 0 },
                disk: String::new(),
            }
        })
        .collect();
    if show_deleted {
        let names: std::collections::HashSet<&str> = here.iter().map(|n| n.name.as_str()).collect();
        for (name, n) in &prev {
            if !names.contains(name) {
                out.push(Entry {
                    name: n.name.clone(),
                    path: join(&n.name),
                    kind: kind(n.kind).into(),
                    size: n.size,
                    mtime: n.mtime / 1_000_000,
                    tag: "deleted".into(),
                    versions: versions.get(*name).copied().unwrap_or(1),
                    items: 0,
                    disk: String::new(),
                });
            }
        }
    }
    if at + 1 == snaps.len() {
        against_disk(core, plan, path, &here, &prev, &versions, &mut out);
    }
    // Folders first, then by name, as Finder does.
    out.sort_by(|a, b| (a.kind != "dir").cmp(&(b.kind != "dir")).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// The newest snapshot's folder against the same folder on the Mac now, so what has gone since
/// the last backup, or come since and not been backed up, shows without waiting for the next one.
/// Only for folders on this Mac; what the plan's rules leave out isn't counted as not backed up.
fn against_disk(
    core: &Core,
    plan: &str,
    path: &str,
    here: &[Node],
    prev: &HashMap<&str, &Node>,
    versions: &HashMap<String, u32>,
    out: &mut Vec<Entry>,
) {
    use crate::config::Place;
    use std::path::{Path, PathBuf};
    let Some(p) = core.config.lock().unwrap().plans.iter().find(|p| p.id == plan).cloned() else { return };
    let folders: Vec<PathBuf> =
        p.sources.iter().filter_map(|s| if let Place::Folder { path, .. } = s { Some(PathBuf::from(path)) } else { None }).collect();
    let local = |e: &str| e.starts_with('/') && folders.iter().any(|f| Path::new(e).starts_with(f));
    for e in out.iter_mut() {
        if local(&e.path)
            && e.tag != "deleted"
            && std::fs::symlink_metadata(&e.path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            e.disk = "gone".into();
        }
    }
    // The top level is the sources themselves; what else is on the Mac there isn't theirs.
    if path.is_empty() || !local(path) {
        return;
    }
    let opts = keepr_engine::backup::Options {
        sources: folders,
        excludes: p.excludes.clone(),
        gitignore: p.gitignore,
        skip_dataless: p.skip_cloud_only,
        skip_marked: p.skip_marked,
        max_file_size: (p.max_file_size > 0).then_some(p.max_file_size),
        skip_paths: core.repo_path(plan).into_iter().collect(),
        ..Default::default()
    };
    let Ok(rules) = keepr_engine::backup::Rules::for_dir(&opts, Path::new(path)) else { return };
    let Ok(rd) = std::fs::read_dir(path) else { return };
    let names: std::collections::HashSet<&str> = here.iter().map(|n| n.name.as_str()).collect();
    for d in rd.flatten() {
        let name = d.file_name().to_string_lossy().to_string();
        if names.contains(name.as_str()) {
            continue;
        }
        let Ok(m) = std::fs::symlink_metadata(d.path()) else { continue };
        if rules.skips(&d.path(), &m) {
            continue;
        }
        // Deleted before the last backup and back since: the same row, now on the Mac.
        if let Some(e) = out.iter_mut().find(|e| e.name == name) {
            e.disk = "unsaved".into();
            continue;
        }
        use std::os::unix::fs::MetadataExt;
        let ft = m.file_type();
        out.push(Entry {
            path: d.path().to_string_lossy().to_string(),
            kind: if ft.is_dir() {
                "dir"
            } else if ft.is_symlink() {
                "link"
            } else {
                "file"
            }
            .into(),
            size: if ft.is_file() { m.size() } else { 0 },
            mtime: m.mtime() * 1000,
            tag: if prev.contains_key(name.as_str()) { "deleted".into() } else { String::new() },
            versions: versions.get(&name).copied().unwrap_or(0),
            items: 0,
            disk: "unsaved".into(),
            name,
        });
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub snapshot: String,
    pub time: String,
    pub size: u64,
    pub mtime: i64,
    pub kept_in: u32,
}

pub fn versions(core: &Core, plan: &str, path: &str) -> Result<Vec<VersionInfo>, String> {
    let repo = core.repo(plan, false)?;
    let snaps = repo.snapshots().map_err(|e| e.0)?;
    Ok(keepr_engine::browse::versions(&repo, &snaps, path)
        .map_err(|e| e.0)?
        .into_iter()
        .map(|v| VersionInfo { snapshot: v.snapshot.hex(), time: v.time, size: v.size, mtime: v.mtime / 1_000_000, kept_in: v.kept_in })
        .collect())
}

/// Restores one file of one snapshot into a temporary folder, for Quick Look; returns where.
pub fn preview_copy(core: &Core, plan: &str, snapshot: &str, path: &str) -> Result<std::path::PathBuf, String> {
    let repo = core.repo(plan, false)?;
    let snap = repo
        .snapshots()
        .map_err(|e| e.0)?
        .into_iter()
        .find(|s| s.id.hex() == snapshot)
        .ok_or("That snapshot is no longer in the backup.")?;
    let dir = std::env::temp_dir().join(format!("keepr-preview-{}", &snapshot[..8]));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let r = keepr_engine::restore::run(
        &repo,
        &snap,
        &[path.to_string()],
        &keepr_engine::restore::Target::Folder(dir.clone()),
        keepr_engine::restore::Conflict::Replace,
        &Default::default(),
    )
    .map_err(|e| e.0)?;
    if r.files == 0 {
        return Err("Nothing to preview.".into());
    }
    keepr_engine::restore::destination(&snap, path, &keepr_engine::restore::Target::Folder(dir)).map_err(|e| e.0)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub plan: String,
    pub plan_name: String,
    pub snapshot: String,
    pub time: String,
    pub gone: bool,
    pub entry: Entry,
}

/// Every plan's backups, every snapshot, by name. Plans whose destination can't be reached are
/// left out, and named in the second list.
pub fn search_everywhere(core: &Core, query: &str) -> (Vec<Found>, Vec<String>) {
    let plans: Vec<(String, String)> = core.config.lock().unwrap().plans.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
    let (mut out, mut missed) = (Vec::new(), Vec::new());
    for (id, name) in plans {
        let res = core.repo(&id, false).and_then(|r| {
            let snaps = r.snapshots().map_err(|e| e.0)?;
            keepr_engine::browse::search_all(&r, &snaps, query, 300).map_err(|e| e.0)
        });
        match res {
            Ok(hits) => out.extend(hits.into_iter().map(|h| Found {
                plan: id.clone(),
                plan_name: name.clone(),
                snapshot: h.snapshot.hex(),
                time: h.time,
                gone: h.gone,
                entry: Entry {
                    name: h.node.name.clone(),
                    path: h.path,
                    kind: kind(h.node.kind).into(),
                    size: h.node.size,
                    mtime: h.node.mtime / 1_000_000,
                    tag: if h.gone { "deleted".into() } else { String::new() },
                    versions: 0,
                    items: 0,
                    disk: String::new(),
                },
            })),
            Err(e) if e.contains("hasn't backed up yet") => {}
            Err(_) => missed.push(name),
        }
    }
    (out, missed)
}

pub fn search(core: &Core, plan: &str, snapshot: &str, query: &str) -> Result<Vec<Entry>, String> {
    let repo = core.repo(plan, false)?;
    let snap = repo
        .snapshots()
        .map_err(|e| e.0)?
        .into_iter()
        .find(|s| s.id.hex() == snapshot)
        .ok_or("That snapshot is no longer in the backup.")?;
    Ok(keepr_engine::browse::search(&repo, &snap, query, 200)
        .map_err(|e| e.0)?
        .into_iter()
        .map(|h| Entry {
            name: h.node.name.clone(),
            path: h.path,
            kind: kind(h.node.kind).into(),
            size: h.node.size,
            mtime: h.node.mtime / 1_000_000,
            tag: String::new(),
            versions: 0,
            items: 0,
            disk: String::new(),
        })
        .collect())
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// "same", "added" (only in the file on this Mac) or "removed" (only in the backup).
    pub kind: String,
    pub text: String,
    pub old: Option<usize>,
    pub new: Option<usize>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    /// Whether the file is still on this Mac.
    pub current_exists: bool,
    pub identical: bool,
    /// Both are text, so `lines` says what changed.
    pub text: bool,
    pub added: usize,
    pub removed: usize,
    /// The changes with a few lines around each; long unchanged stretches left out.
    pub lines: Vec<Line>,
    pub backup_size: u64,
    pub current_size: u64,
}

/// A version from the backup against the file as it is now.
pub fn compare(core: &Core, plan: &str, snapshot: &str, path: &str) -> Result<Comparison, String> {
    let repo = core.repo(plan, false)?;
    let snap = repo
        .snapshots()
        .map_err(|e| e.0)?
        .into_iter()
        .find(|s| s.id.hex() == snapshot)
        .ok_or("That snapshot is no longer in the backup.")?;
    let node = keepr_engine::browse::node_at(&repo, &snap, path).map_err(|e| e.0)?.ok_or("That file isn't in this snapshot.")?;
    if node.kind != NodeKind::File {
        return Err("Only files can be compared.".into());
    }
    let mut old = Vec::with_capacity(node.size as usize);
    for c in &node.content {
        old.extend(repo.load(c).map_err(|e| e.0)?);
    }
    let current = std::fs::read(path).ok();
    let mut out = Comparison {
        current_exists: current.is_some(),
        identical: current.as_deref() == Some(&old[..]),
        text: false,
        added: 0,
        removed: 0,
        lines: vec![],
        backup_size: old.len() as u64,
        current_size: current.as_ref().map_or(0, |c| c.len() as u64),
    };
    let (Some(new), Ok(a)) = (current.as_ref(), std::str::from_utf8(&old)) else { return Ok(out) };
    let Ok(b) = std::str::from_utf8(new) else { return Ok(out) };
    if old.len() > 4 << 20 || new.len() > 4 << 20 {
        return Ok(out);
    }
    out.text = true;
    let diff = similar::TextDiff::from_lines(a, b);
    for group in diff.grouped_ops(3) {
        for op in group {
            for ch in diff.iter_changes(&op) {
                let kind = match ch.tag() {
                    similar::ChangeTag::Equal => "same",
                    similar::ChangeTag::Insert => {
                        out.added += 1;
                        "added"
                    }
                    similar::ChangeTag::Delete => {
                        out.removed += 1;
                        "removed"
                    }
                };
                out.lines.push(Line {
                    kind: kind.into(),
                    text: ch.value().trim_end_matches('\n').to_string(),
                    old: ch.old_index().map(|i| i + 1),
                    new: ch.new_index().map(|i| i + 1),
                });
            }
        }
        out.lines.push(Line { kind: "gap".into(), text: String::new(), old: None, new: None });
    }
    out.lines.pop();
    Ok(out)
}
