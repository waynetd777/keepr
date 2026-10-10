// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting files back, where they were or into another folder.
//!
//! Each file is written to a temporary name beside its destination and renamed into place when
//! it is complete, so a cancelled or failed restore never leaves half a file where a whole one
//! was. Modified times and permissions are restored too.

use crate::backup::Control;
use crate::browse::node_at;
use crate::repo::{Repo, Snapshot};
use crate::tree::{Node, NodeKind};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "path")]
pub enum Target {
    Original,
    /// Items keep their folders inside this one: /Users/w/Documents/Finance/x restores to
    /// <folder>/Documents/Finance/x.
    Folder(PathBuf),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    /// Keep the file that's there and restore beside it as "name (restored).ext".
    KeepBoth,
    Replace,
    Skip,
}

#[derive(Default, Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub renamed: u64,
    pub errors: Vec<String>,
}

pub fn destination(snap: &Snapshot, path: &str, target: &Target) -> Result<PathBuf> {
    match target {
        // A bucket was only read from: Keepr has no leave to write there.
        Target::Original if path.contains("://") => Err(Error::new("Files from a bucket can only be restored into a folder. Choose one.")),
        Target::Original => Ok(PathBuf::from(path)),
        Target::Folder(dir) => {
            let (src, _) = crate::browse::split(snap, path).ok_or_else(|| Error::new(format!("{path} isn't in this snapshot")))?;
            let parent = Path::new(&src).parent().unwrap_or(Path::new("/"));
            let rel = Path::new(path).strip_prefix(parent).map_err(|_| Error::new(format!("{path} isn't in this snapshot")))?;
            Ok(dir.join(rel))
        }
    }
}

/// "Budget.numbers" → "Budget (restored).numbers", then "(restored 2)" and so on.
pub fn free_name(p: &Path) -> PathBuf {
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let dir = p.parent().unwrap_or(Path::new("/"));
    for n in 1.. {
        let tag = if n == 1 { "restored".to_string() } else { format!("restored {n}") };
        let cand = dir.join(format!("{stem} ({tag}){ext}"));
        if fs::symlink_metadata(&cand).is_err() {
            return cand;
        }
    }
    unreachable!()
}

/// Whether `name` is one file's name: not empty, `.` or `..`, and without a '/'.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

struct Job<'a> {
    repo: &'a Repo,
    ctl: &'a Control,
    conflict: Conflict,
    out: Restored,
}

fn set_times(p: &Path, n: &Node) {
    let t = filetime::FileTime::from_unix_time(n.mtime.div_euclid(1_000_000_000), n.mtime.rem_euclid(1_000_000_000) as u32);
    let _ = filetime::set_symlink_file_times(p, t, t);
}

impl Job<'_> {
    fn err(&mut self, p: &Path, e: impl std::fmt::Display) {
        if self.out.errors.len() < 50 {
            self.out.errors.push(format!("{}: {e}", p.display()));
        }
    }

    fn restore(&mut self, n: &Node, dest: &Path) -> Result<()> {
        self.ctl.checkpoint()?;
        match n.kind {
            NodeKind::Dir => {
                if let Err(e) = fs::create_dir_all(dest) {
                    self.err(dest, e);
                    return Ok(());
                }
                if let Some(t) = n.subtree {
                    let tree = self.repo.load_tree(&t)?;
                    for child in &tree.nodes {
                        // A name comes from the snapshot, and the snapshot from a bucket's keys
                        // perhaps: never one that would put a file outside `dest`.
                        if !safe_name(&child.name) {
                            self.err(&dest.join(&child.name), "a name that can't be a file's, so it's left out");
                            continue;
                        }
                        self.restore(child, &dest.join(&child.name))?;
                    }
                }
                let _ = fs::set_permissions(dest, fs::Permissions::from_mode(n.mode | 0o700));
                set_times(dest, n);
            }
            NodeKind::File => {
                let mut dest = dest.to_path_buf();
                if fs::symlink_metadata(&dest).is_ok() {
                    match self.conflict {
                        Conflict::Skip => {
                            self.out.skipped += 1;
                            return Ok(());
                        }
                        Conflict::KeepBoth => {
                            dest = free_name(&dest);
                            self.out.renamed += 1;
                        }
                        Conflict::Replace => {}
                    }
                }
                if let Err(e) = self.write_file(n, &dest) {
                    if e == crate::cancelled() {
                        return Err(e);
                    }
                    self.err(&dest, e);
                }
            }
            NodeKind::Symlink => {
                if fs::symlink_metadata(dest).is_ok() {
                    match self.conflict {
                        Conflict::Replace => {
                            let _ = fs::remove_file(dest);
                        }
                        _ => {
                            self.out.skipped += 1;
                            return Ok(());
                        }
                    }
                }
                if let Some(t) = &n.target {
                    if let Err(e) = std::os::unix::fs::symlink(t, dest) {
                        self.err(dest, e);
                    }
                }
            }
        }
        Ok(())
    }

    fn write_file(&mut self, n: &Node, dest: &Path) -> Result<()> {
        let dir = dest.parent().ok_or_else(|| Error::new("no folder"))?;
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".{}.keepr-{}", dest.file_name().unwrap().to_string_lossy(), crate::Id::random().short()));
        let res = (|| -> Result<()> {
            let mut f = fs::File::create(&tmp)?;
            for c in &n.content {
                self.ctl.checkpoint()?;
                let data = self.repo.load(c)?;
                f.write_all(&data)?;
                self.ctl.progress.bytes_read.fetch_add(data.len() as u64, Relaxed);
            }
            crate::backend::sync(&f)?;
            drop(f);
            fs::set_permissions(&tmp, fs::Permissions::from_mode(n.mode))?;
            set_times(&tmp, n);
            fs::rename(&tmp, dest)?;
            Ok(())
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        res?;
        self.out.files += 1;
        self.out.bytes += n.size;
        self.ctl.progress.files_read.fetch_add(1, Relaxed);
        *self.ctl.progress.current.lock().unwrap() = dest.to_string_lossy().to_string();
        Ok(())
    }
}

/// What restoring these items will write: files and bytes, for progress.
pub fn measure(repo: &Repo, snap: &Snapshot, items: &[String]) -> Result<(u64, u64)> {
    let (mut files, mut bytes) = (0, 0);
    for p in items {
        match node_at(repo, snap, p)? {
            Some(Node { kind: NodeKind::Dir, subtree: Some(t), .. }) => {
                let (b, f) = crate::browse::du(repo, &t)?;
                bytes += b;
                files += f;
            }
            Some(n) if n.kind == NodeKind::File => {
                bytes += n.size;
                files += 1;
            }
            _ => {}
        }
    }
    Ok((files, bytes))
}

pub fn run(repo: &Repo, snap: &Snapshot, items: &[String], target: &Target, conflict: Conflict, ctl: &Control) -> Result<Restored> {
    let (files, bytes) = measure(repo, snap, items)?;
    ctl.progress.files_to_read.store(files, Relaxed);
    ctl.progress.bytes_to_read.store(bytes, Relaxed);
    let mut job = Job { repo, ctl, conflict, out: Restored::default() };
    for p in items {
        let node = node_at(repo, snap, p)?.ok_or_else(|| Error::new(format!("{p} isn't in this snapshot")))?;
        let dest = destination(snap, p, target)?;
        job.restore(&node, &dest)?;
    }
    Ok(job.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{self, tests::*};

    #[test]
    fn names_that_leave_the_folder_are_refused() {
        for bad in ["", ".", "..", "a/b", "../x"] {
            assert!(!safe_name(bad), "{bad:?}");
        }
        assert!(safe_name("..hidden") && safe_name("a b.txt"));
    }

    #[test]
    fn restores_to_a_folder_and_handles_conflicts() {
        let (src, _d, repo) = setup(Some("pw"));
        let base = src.path().to_string_lossy().to_string();
        fs::create_dir_all(src.path().join("docs")).unwrap();
        fs::write(src.path().join("docs/a.txt"), vec![9u8; 2_500_000]).unwrap();
        fs::write(src.path().join("top.txt"), b"top").unwrap();
        std::os::unix::fs::symlink("top.txt", src.path().join("link")).unwrap();
        let snap = backup::run(&repo, &opts(src.path()), None, &Control::default()).unwrap();

        let out = tempfile::tempdir().unwrap();
        let r = run(&repo, &snap, std::slice::from_ref(&base), &Target::Folder(out.path().into()), Conflict::KeepBoth, &Control::default())
            .unwrap();
        let name = src.path().file_name().unwrap();
        assert_eq!(r.files, 2);
        assert_eq!(fs::read(out.path().join(name).join("docs/a.txt")).unwrap(), vec![9u8; 2_500_000]);
        assert_eq!(fs::read_link(out.path().join(name).join("link")).unwrap(), Path::new("top.txt"));
        let m1 = fs::metadata(src.path().join("top.txt")).unwrap().modified().unwrap();
        let m2 = fs::metadata(out.path().join(name).join("top.txt")).unwrap().modified().unwrap();
        assert_eq!(m1, m2);

        // Back to the original place, where the file has since changed.
        fs::write(src.path().join("top.txt"), b"edited").unwrap();
        let item = format!("{base}/top.txt");
        let r = run(&repo, &snap, std::slice::from_ref(&item), &Target::Original, Conflict::KeepBoth, &Control::default()).unwrap();
        assert_eq!(r.renamed, 1);
        assert_eq!(fs::read(src.path().join("top (restored).txt")).unwrap(), b"top");
        assert_eq!(fs::read(src.path().join("top.txt")).unwrap(), b"edited");
        let r = run(&repo, &snap, std::slice::from_ref(&item), &Target::Original, Conflict::Skip, &Control::default()).unwrap();
        assert_eq!(r.skipped, 1);
        run(&repo, &snap, &[item], &Target::Original, Conflict::Replace, &Control::default()).unwrap();
        assert_eq!(fs::read(src.path().join("top.txt")).unwrap(), b"top");
    }
}
