//! Looking inside snapshots: a folder's contents, a file's versions across snapshots, and search.
//!
//! Paths are the files' original absolute paths ("/Users/wayne/Documents/Budget.numbers"). The
//! empty path is a snapshot's top level: one entry per source.

use crate::repo::{Repo, Snapshot};
use crate::tree::{Node, NodeKind, Tree};
use crate::{Id, Result};
use serde::Serialize;
use std::sync::Arc;

/// Which source a path is in, and the names below it.
fn split<'a>(snap: &Snapshot, path: &'a str) -> Option<(String, Vec<&'a str>)> {
    let path = path.trim_end_matches('/');
    let mut best: Option<&String> = None;
    for s in &snap.sources {
        let s_trim = s.trim_end_matches('/');
        if path == s_trim || path.starts_with(&format!("{s_trim}/")) {
            if best.is_none_or(|b| s.len() > b.len()) {
                best = Some(s);
            }
        }
    }
    let src = best?;
    let rest = path[src.trim_end_matches('/').len()..].trim_start_matches('/');
    Some((src.clone(), if rest.is_empty() { vec![] } else { rest.split('/').collect() }))
}

pub fn node_at(repo: &Repo, snap: &Snapshot, path: &str) -> Result<Option<Node>> {
    let Some((src, names)) = split(snap, path) else { return Ok(None) };
    let root = repo.load_tree(&snap.tree)?;
    let Some(mut node) = root.get(&src).cloned() else { return Ok(None) };
    for name in names {
        let Some(sub) = node.subtree else { return Ok(None) };
        let t = repo.load_tree(&sub)?;
        match t.get(name) {
            Some(n) => node = n.clone(),
            None => return Ok(None),
        }
    }
    Ok(Some(node))
}

/// A folder's entries in a snapshot, or the sources for the empty path.
pub fn list(repo: &Repo, snap: &Snapshot, path: &str) -> Result<Vec<Node>> {
    if path.is_empty() {
        return Ok(repo.load_tree(&snap.tree)?.nodes.clone());
    }
    match node_at(repo, snap, path)? {
        Some(Node { subtree: Some(t), .. }) => Ok(repo.load_tree(&t)?.nodes.clone()),
        _ => Ok(vec![]),
    }
}

/// Total size and file count below a folder, for the restore screen.
pub fn du(repo: &Repo, tree: &Id) -> Result<(u64, u64)> {
    let t = repo.load_tree(tree)?;
    let (mut bytes, mut files) = (0, 0);
    for n in &t.nodes {
        match (n.kind, n.subtree) {
            (NodeKind::Dir, Some(s)) => {
                let (b, f) = du(repo, &s)?;
                bytes += b;
                files += f;
            }
            (NodeKind::File, _) => {
                bytes += n.size;
                files += 1;
            }
            _ => {}
        }
    }
    Ok((bytes, files))
}

#[derive(Serialize, Clone, Debug)]
pub struct Version {
    pub snapshot: Id,
    pub time: String,
    pub size: u64,
    pub mtime: i64,
    /// How many later snapshots kept this same version.
    pub kept_in: u32,
}

/// A path's distinct versions, newest first: a new entry each time its contents changed.
pub fn versions(repo: &Repo, snaps: &[Snapshot], path: &str) -> Result<Vec<Version>> {
    let mut out: Vec<Version> = Vec::new();
    let mut last_key: Option<String> = None;
    let mut ordered: Vec<&Snapshot> = snaps.iter().collect();
    ordered.sort_by(|a, b| a.time.cmp(&b.time));
    for s in ordered {
        match node_at(repo, s, path)? {
            Some(n) => {
                let key = n.content_key();
                if last_key.as_deref() == Some(key.as_str()) {
                    if let Some(v) = out.last_mut() {
                        v.kept_in += 1;
                    }
                } else {
                    out.push(Version { snapshot: s.id, time: s.time.clone(), size: n.size, mtime: n.mtime, kept_in: 0 });
                    last_key = Some(key);
                }
            }
            None => last_key = None,
        }
    }
    out.reverse();
    Ok(out)
}

#[derive(Serialize, Clone, Debug)]
pub struct Hit {
    pub path: String,
    pub node: Node,
}

/// Files and folders whose names contain `query` (any case), in one snapshot, up to `limit`.
pub fn search(repo: &Repo, snap: &Snapshot, query: &str, limit: usize) -> Result<Vec<Hit>> {
    let q = query.to_lowercase();
    let mut out = Vec::new();
    fn walk(repo: &Repo, t: Arc<Tree>, base: &str, q: &str, limit: usize, out: &mut Vec<Hit>) -> Result<()> {
        for n in &t.nodes {
            if out.len() >= limit {
                return Ok(());
            }
            let path = if base.is_empty() { n.name.clone() } else { format!("{base}/{}", n.name) };
            if !base.is_empty() && n.name.to_lowercase().contains(q) {
                out.push(Hit { path: path.clone(), node: n.clone() });
            }
            if let Some(s) = n.subtree {
                walk(repo, repo.load_tree(&s)?, &path, q, limit, out)?;
            }
        }
        Ok(())
    }
    walk(repo, repo.load_tree(&snap.tree)?, "", &q, limit, &mut out)?;
    Ok(out)
}

#[derive(Serialize, Clone, Debug)]
pub struct FoundAnywhere {
    pub path: String,
    pub node: Node,
    /// The newest snapshot that has it.
    pub snapshot: Id,
    pub time: String,
    /// Not in the newest snapshot: deleted, renamed or moved since.
    pub gone: bool,
}

/// Names containing `query` (any case) in any snapshot, each path once, from the newest snapshot
/// that has it. A folder tree shared by many snapshots is read once.
pub fn search_all(repo: &Repo, snaps: &[Snapshot], query: &str, limit: usize) -> Result<Vec<FoundAnywhere>> {
    let q = query.to_lowercase();
    let mut ordered: Vec<&Snapshot> = snaps.iter().collect();
    ordered.sort_by(|a, b| b.time.cmp(&a.time));
    let mut seen_trees = std::collections::HashSet::new();
    let mut found: std::collections::HashMap<String, FoundAnywhere> = std::collections::HashMap::new();
    let newest = ordered.first().map(|s| s.id);
    for s in ordered {
        let mut stack: Vec<(Id, String)> = vec![(s.tree, String::new())];
        while let Some((t, base)) = stack.pop() {
            if !seen_trees.insert(t) {
                continue;
            }
            for n in &repo.load_tree(&t)?.nodes {
                let path = if base.is_empty() { n.name.clone() } else { format!("{base}/{}", n.name) };
                if !base.is_empty() && n.name.to_lowercase().contains(&q) && !found.contains_key(&path) && found.len() < limit {
                    found.insert(path.clone(), FoundAnywhere { path: path.clone(), node: n.clone(), snapshot: s.id, time: s.time.clone(), gone: Some(s.id) != newest });
                }
                if let Some(sub) = n.subtree {
                    stack.push((sub, path));
                }
            }
        }
    }
    // A path found first in an older snapshot's tree may still be in the newest one under a
    // folder that was read earlier; check before calling it gone.
    if let Some(n) = snaps.iter().max_by(|a, b| a.time.cmp(&b.time)) {
        for f in found.values_mut().filter(|f| f.gone) {
            if node_at(repo, n, &f.path)?.is_some() {
                f.gone = false;
            }
        }
    }
    let mut out: Vec<FoundAnywhere> = found.into_values().collect();
    out.sort_by(|a, b| a.gone.cmp(&b.gone).then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase())));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{run, tests::*, Control};
    use std::fs;

    #[test]
    fn lists_and_finds_versions() {
        let (src, _d, repo) = setup(None);
        let base = src.path().to_string_lossy().to_string();
        fs::create_dir_all(src.path().join("docs")).unwrap();
        fs::write(src.path().join("docs/budget.txt"), b"v1").unwrap();
        let s1 = run(&repo, &opts(src.path()), None, &Control::default()).unwrap();
        let s2 = run(&repo, &opts(src.path()), Some(&s1), &Control::default()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(src.path().join("docs/budget.txt"), b"version 2").unwrap();
        let s3 = run(&repo, &opts(src.path()), Some(&s2), &Control::default()).unwrap();

        assert_eq!(list(&repo, &s3, "").unwrap()[0].name, base);
        let docs = list(&repo, &s3, &format!("{base}/docs")).unwrap();
        assert_eq!(docs[0].name, "budget.txt");
        assert!(node_at(&repo, &s3, &format!("{base}/nope")).unwrap().is_none());
        assert!(node_at(&repo, &s3, "/elsewhere").unwrap().is_none());

        let snaps = repo.snapshots().unwrap();
        let v = versions(&repo, &snaps, &format!("{base}/docs/budget.txt")).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].size, 9);
        assert_eq!(v[1].kept_in, 1);

        fs::write(src.path().join("docs/old-notes.txt"), b"x").unwrap();
        let s4 = run(&repo, &opts(src.path()), Some(&s3), &Control::default()).unwrap();
        fs::remove_file(src.path().join("docs/old-notes.txt")).unwrap();
        run(&repo, &opts(src.path()), Some(&s4), &Control::default()).unwrap();
        let all = search_all(&repo, &repo.snapshots().unwrap(), "notes", 10).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].gone && all[0].snapshot == s4.id);
        let all = search_all(&repo, &repo.snapshots().unwrap(), "budget", 10).unwrap();
        assert!(!all[0].gone);
        let hits = search(&repo, &s3, "BUDGET", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("docs/budget.txt"));
        assert_eq!(du(&repo, &list(&repo, &s3, "").unwrap()[0].subtree.unwrap()).unwrap(), (9, 1));
    }
}
