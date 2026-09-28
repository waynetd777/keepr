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
    SnapInfo { id: s.id.hex(), time: s.time.clone(), kind: s.kind.clone(), files: s.stats.files, bytes: s.stats.bytes, changed: s.stats.new_files + s.stats.changed_files, added_bytes: s.stats.added_bytes }
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
    let before: Option<Vec<Node>> = if at > 0 { trees[at - 1].map(|t| repo.load_tree(&t).map(|t| t.nodes.clone())).transpose().map_err(|e| e.0)? } else { None };
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
            }
        })
        .collect();
    if show_deleted {
        let names: std::collections::HashSet<&str> = here.iter().map(|n| n.name.as_str()).collect();
        for (name, n) in &prev {
            if !names.contains(name) {
                out.push(Entry { name: n.name.clone(), path: join(&n.name), kind: kind(n.kind).into(), size: n.size, mtime: n.mtime / 1_000_000, tag: "deleted".into(), versions: versions.get(*name).copied().unwrap_or(1), items: 0 });
            }
        }
    }
    // Folders first, then by name, as Finder does.
    out.sort_by(|a, b| (a.kind != "dir").cmp(&(b.kind != "dir")).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
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

/// Restores one file of one snapshot into a temporary folder and opens Quick Look on it.
pub fn quick_look(core: &Core, plan: &str, snapshot: &str, path: &str) -> Result<(), String> {
    let repo = core.repo(plan, false)?;
    let snap = repo.snapshots().map_err(|e| e.0)?.into_iter().find(|s| s.id.hex() == snapshot).ok_or("That snapshot is no longer in the backup.")?;
    let dir = std::env::temp_dir().join(format!("keepr-preview-{}", &snapshot[..8]));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let r = keepr_engine::restore::run(&repo, &snap, &[path.to_string()], &keepr_engine::restore::Target::Folder(dir.clone()), keepr_engine::restore::Conflict::Replace, &Default::default()).map_err(|e| e.0)?;
    if r.files == 0 {
        return Err("Nothing to preview.".into());
    }
    let dest = keepr_engine::restore::destination(&snap, path, &keepr_engine::restore::Target::Folder(dir)).map_err(|e| e.0)?;
    std::process::Command::new("/usr/bin/qlmanage").arg("-p").arg(&dest).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn search(core: &Core, plan: &str, snapshot: &str, query: &str) -> Result<Vec<Entry>, String> {
    let repo = core.repo(plan, false)?;
    let snap = repo.snapshots().map_err(|e| e.0)?.into_iter().find(|s| s.id.hex() == snapshot).ok_or("That snapshot is no longer in the backup.")?;
    Ok(keepr_engine::browse::search(&repo, &snap, query, 200)
        .map_err(|e| e.0)?
        .into_iter()
        .map(|h| Entry { name: h.node.name.clone(), path: h.path, kind: kind(h.node.kind).into(), size: h.node.size, mtime: h.node.mtime / 1_000_000, tag: String::new(), versions: 0, items: 0 })
        .collect())
}
