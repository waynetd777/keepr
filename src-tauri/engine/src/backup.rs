//! Taking a snapshot.
//!
//! Three passes:
//!
//! 1. **Look**: walk the sources, apply the exclusions, and compare each file with the previous
//!    snapshot. A file whose size, modified time, status-change time and inode all match keeps
//!    the previous snapshot's chunks without being read. (A full backup skips this comparison and
//!    re-reads everything; chunks already stored are still not stored again.)
//! 2. **Read**: read the files that need it, in parallel, chunk them and store new chunks.
//! 3. **Commit**: write the trees bottom-up, flush the last pack and the index, and write the
//!    snapshot last. Nothing half-done ever counts as a snapshot.
//!
//! Sources are only ever opened for reading.

use crate::repo::{Kind, Repo, Snapshot, Stats};
use crate::tree::{Node, NodeKind, Tree};
use crate::{cancelled, Error, Id, Result};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub plan: String,
    pub sources: Vec<PathBuf>,
    /// Patterns: `*.tmp` matches a name anywhere, `node_modules/` a folder anywhere, and a path
    /// starting `/` or `~/` that path and everything in it.
    pub excludes: Vec<String>,
    /// Leave out what .gitignore files say to.
    pub gitignore: bool,
    /// Leave out files that are only in the cloud (OneDrive, iCloud Drive placeholders), rather
    /// than make macOS download them to read them.
    pub skip_dataless: bool,
    pub max_file_size: Option<u64>,
    /// Re-read every file instead of trusting unchanged metadata.
    pub full: bool,
    /// Paths never backed up whatever the rules say (the repository itself, when it is inside a source).
    pub skip_paths: Vec<PathBuf>,
    /// Where to read each source from, when not the source itself: a mounted APFS snapshot of its
    /// volume, so every file is read as it was at one moment. The snapshot keeps the original paths.
    pub read_from: Vec<Option<PathBuf>>,
    /// Folders macOS says changed since the previous snapshot. Any other folder is taken from the
    /// previous snapshot without being looked at. None: look at everything.
    pub changes: Option<Changes>,
}

/// What changed since the previous snapshot, from macOS's record of file-system events.
#[derive(Clone, Debug, Default)]
pub struct Changes {
    /// Folders with changes, and every folder above them.
    pub touched: HashSet<PathBuf>,
    /// Folders to look through in full (macOS lost track of the details below them).
    pub rescan: Vec<PathBuf>,
}

impl Changes {
    pub fn from_events(paths: impl IntoIterator<Item = PathBuf>, rescan: Vec<PathBuf>) -> Changes {
        let mut touched = HashSet::new();
        for p in paths {
            let mut cur = Some(p.as_path());
            while let Some(c) = cur {
                if !touched.insert(c.to_path_buf()) {
                    break; // its parents are in already
                }
                cur = c.parent();
            }
        }
        for r in &rescan {
            let mut cur = Some(r.as_path());
            while let Some(c) = cur {
                touched.insert(c.to_path_buf());
                cur = c.parent();
            }
        }
        Changes { touched, rescan }
    }

    /// Whether a folder can be taken from the previous snapshot as it was.
    pub fn unchanged(&self, dir: &Path) -> bool {
        !self.touched.contains(dir) && !self.rescan.iter().any(|r| dir.starts_with(r))
    }
}

pub const PHASE_LOOK: u8 = 0;
pub const PHASE_READ: u8 = 1;
pub const PHASE_COMMIT: u8 = 2;

#[derive(Default)]
pub struct Progress {
    pub phase: AtomicU8,
    pub files_seen: AtomicU64,
    pub files_to_read: AtomicU64,
    pub files_read: AtomicU64,
    pub bytes_to_read: AtomicU64,
    pub bytes_read: AtomicU64,
    pub current: Mutex<String>,
}

/// Shared between a running job and whoever watches it.
#[derive(Default)]
pub struct Control {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
    pub progress: Progress,
}

impl Control {
    /// Waits while paused; errors once cancelled.
    pub fn checkpoint(&self) -> Result<()> {
        while self.pause.load(Relaxed) && !self.cancel.load(Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if self.cancel.load(Relaxed) {
            return Err(cancelled());
        }
        Ok(())
    }
}

/// macOS's flag for a file whose contents are in the cloud only.
const SF_DATALESS: u32 = 0x4000_0000;

struct Excluder {
    names: globset::GlobSet,
    dir_names: globset::GlobSet,
    paths: globset::GlobSet,
}

impl Excluder {
    fn new(patterns: &[String]) -> Result<Excluder> {
        let home = std::env::var("HOME").unwrap_or_default();
        let (mut names, mut dirs, mut paths) = (globset::GlobSetBuilder::new(), globset::GlobSetBuilder::new(), globset::GlobSetBuilder::new());
        let glob = |p: &str| globset::GlobBuilder::new(p).literal_separator(true).build().map_err(|e| Error::new(format!("The rule {p:?} isn't valid: {e}")));
        for p in patterns.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            let p = if let Some(rest) = p.strip_prefix("~/") { format!("{home}/{rest}") } else { p.to_string() };
            if p.starts_with('/') {
                let base = p.trim_end_matches('/');
                paths.add(glob(base)?);
                paths.add(glob(&format!("{base}/**"))?);
            } else if let Some(d) = p.strip_suffix('/') {
                dirs.add(glob(d)?);
            } else {
                names.add(glob(&p)?);
            }
        }
        let b = |s: globset::GlobSetBuilder| s.build().map_err(|e| Error::new(e.to_string()));
        Ok(Excluder { names: b(names)?, dir_names: b(dirs)?, paths: b(paths)? })
    }

    fn excluded(&self, path: &Path, name: &str, is_dir: bool) -> bool {
        self.names.is_match(name) || (is_dir && self.dir_names.is_match(name)) || self.paths.is_match(path)
    }
}

enum Entry {
    Dir(ScanDir),
    /// A file, and its place in the read list when it needs reading.
    File(Node, Option<usize>),
    Link(Node),
    /// A folder taken whole from the previous snapshot.
    Kept(Node),
}

struct ScanDir {
    node: Node,
    entries: Vec<Entry>,
}

struct Job {
    path: PathBuf,
    /// The path as people know it (not inside a snapshot's mount).
    shown: PathBuf,
    size: u64,
    /// The previous snapshot's version, kept if this one can't be read.
    previous: Option<Node>,
}

struct Walk<'a> {
    repo: &'a Repo,
    opts: &'a Options,
    ctl: &'a Control,
    ex: Excluder,
    jobs: Vec<Job>,
    stats: Stats,
    ignores: Vec<ignore::gitignore::Gitignore>,
    /// (read from, shown as): a source read through a snapshot's mount.
    map: Option<(PathBuf, PathBuf)>,
}

fn node_from(name: String, kind: NodeKind, m: &fs::Metadata) -> Node {
    Node {
        name,
        kind,
        size: if kind == NodeKind::File { m.size() } else { 0 },
        mtime: m.mtime() * 1_000_000_000 + m.mtime_nsec(),
        ctime: m.ctime() * 1_000_000_000 + m.ctime_nsec(),
        inode: m.ino(),
        files: 0,
        mode: m.mode() & 0o7777,
        content: vec![],
        subtree: None,
        target: None,
    }
}

impl Walk<'_> {
    fn logical(&self, p: &Path) -> PathBuf {
        match &self.map {
            Some((from, to)) => p.strip_prefix(from).map(|rest| to.join(rest)).unwrap_or_else(|_| p.to_path_buf()),
            None => p.to_path_buf(),
        }
    }

    /// A folder unchanged since the previous snapshot, by macOS's record.
    fn keep(&mut self, logical: &Path, prev: Option<&Node>) -> Option<Entry> {
        let ch = self.opts.changes.as_ref()?;
        let p = prev?;
        if self.opts.full || p.kind != NodeKind::Dir || p.subtree.is_none() || !ch.unchanged(logical) {
            return None;
        }
        self.stats.dirs += 1;
        self.stats.files += p.files;
        self.stats.bytes += p.size;
        Some(Entry::Kept(p.clone()))
    }

    fn note(&mut self, msg: String) {
        self.stats.error_count += 1;
        if self.stats.errors.len() < 50 {
            self.stats.errors.push(msg);
        }
    }

    fn gitignored(&self, path: &Path, is_dir: bool) -> bool {
        for g in self.ignores.iter().rev() {
            match g.matched(path, is_dir) {
                ignore::Match::Ignore(_) => return true,
                ignore::Match::Whitelist(_) => return false,
                ignore::Match::None => {}
            }
        }
        false
    }

    fn file(&mut self, name: String, path: &Path, m: &fs::Metadata, prev: Option<&Node>) -> Option<Entry> {
        if self.opts.skip_dataless && std::os::macos::fs::MetadataExt::st_flags(m) & SF_DATALESS != 0 {
            return None;
        }
        if self.opts.max_file_size.is_some_and(|max| m.size() > max) {
            return None;
        }
        let mut node = node_from(name, NodeKind::File, m);
        self.stats.files += 1;
        self.stats.bytes += node.size;
        let unchanged = !self.opts.full
            && prev.is_some_and(|p| p.kind == NodeKind::File && p.size == node.size && p.mtime == node.mtime && p.ctime == node.ctime && p.inode == node.inode)
            && prev.is_some_and(|p| p.content.iter().all(|c| self.repo.has(c)));
        if unchanged {
            node.content = prev.unwrap().content.clone();
            return Some(Entry::File(node, None));
        }
        if prev.is_none() {
            self.stats.new_files += 1;
        } else if prev.is_some_and(|p| p.size != node.size || p.mtime != node.mtime || p.inode != node.inode) {
            self.stats.changed_files += 1;
        }
        self.ctl.progress.files_to_read.fetch_add(1, Relaxed);
        self.ctl.progress.bytes_to_read.fetch_add(node.size, Relaxed);
        let shown = self.logical(path);
        self.jobs.push(Job { path: path.to_path_buf(), shown, size: node.size, previous: prev.cloned() });
        Some(Entry::File(node, Some(self.jobs.len() - 1)))
    }

    fn dir(&mut self, name: String, path: &Path, m: &fs::Metadata, prev: Option<Arc<Tree>>) -> Result<ScanDir> {
        self.ctl.checkpoint()?;
        self.stats.dirs += 1;
        let node = node_from(name, NodeKind::Dir, m);
        let pushed = if self.opts.gitignore && path.join(".gitignore").is_file() {
            let mut b = ignore::gitignore::GitignoreBuilder::new(path);
            b.add(path.join(".gitignore"));
            match b.build() {
                Ok(g) => {
                    self.ignores.push(g);
                    true
                }
                Err(_) => false,
            }
        } else {
            false
        };
        let mut names: Vec<(String, PathBuf)> = match fs::read_dir(path) {
            Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| (e.file_name().to_string_lossy().to_string(), e.path())).collect(),
            Err(e) => {
                self.note(format!("{}: {e}", self.logical(path).display()));
                vec![]
            }
        };
        names.sort();
        let mut entries = Vec::with_capacity(names.len());
        for (name, p) in names {
            let lp = self.logical(&p);
            if self.opts.skip_paths.iter().any(|s| lp.starts_with(s)) {
                continue;
            }
            let m = match fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(e) => {
                    self.note(format!("{}: {e}", lp.display()));
                    continue;
                }
            };
            let ft = m.file_type();
            let is_dir = ft.is_dir();
            if self.ex.excluded(&lp, &name, is_dir) || (self.opts.gitignore && self.gitignored(&p, is_dir)) {
                continue;
            }
            self.ctl.progress.files_seen.fetch_add(1, Relaxed);
            let prev_node = prev.as_ref().and_then(|t| t.get(&name)).cloned();
            if is_dir {
                if let Some(kept) = self.keep(&lp, prev_node.as_ref()) {
                    entries.push(kept);
                    continue;
                }
                let prev_tree = match prev_node.as_ref().and_then(|n| n.subtree) {
                    Some(id) => self.repo.load_tree(&id).ok(),
                    None => None,
                };
                entries.push(Entry::Dir(self.dir(name, &p, &m, prev_tree)?));
            } else if ft.is_file() {
                if let Some(e) = self.file(name, &p, &m, prev_node.as_ref()) {
                    entries.push(e);
                }
            } else if ft.is_symlink() {
                let mut n = node_from(name, NodeKind::Symlink, &m);
                n.target = fs::read_link(&p).ok().map(|t| t.to_string_lossy().to_string());
                entries.push(Entry::Link(n));
            }
            // Sockets, pipes and devices aren't files anyone restores.
        }
        if pushed {
            self.ignores.pop();
        }
        Ok(ScanDir { node, entries })
    }
}

/// Reads a file and stores its chunks.
fn read_file(repo: &Repo, ctl: &Control, job: &Job) -> Result<Vec<Id>> {
    let f = fs::File::open(&job.path)?;
    let c = &repo.config.chunker;
    let mut ids = Vec::new();
    for chunk in fastcdc::v2020::StreamCDC::new(f, c.min, c.avg, c.max) {
        ctl.checkpoint()?;
        let chunk = chunk.map_err(|e| Error::new(e.to_string()))?;
        ctl.progress.bytes_read.fetch_add(chunk.length as u64, Relaxed);
        ids.push(repo.add(Kind::Data, &chunk.data)?.0);
    }
    Ok(ids)
}

pub fn run(repo: &Arc<Repo>, opts: &Options, parent: Option<&Snapshot>, ctl: &Control) -> Result<Snapshot> {
    let started = Instant::now();
    let _lock = repo.lock(false)?;
    let before = (repo.counters.added_bytes.load(Relaxed), repo.counters.stored_bytes.load(Relaxed), repo.counters.dup_bytes.load(Relaxed));

    // 1. Look.
    ctl.progress.phase.store(PHASE_LOOK, Relaxed);
    let prev_root = match parent {
        Some(p) => Some(repo.load_tree(&p.tree)?),
        None => None,
    };
    let mut w = Walk { repo, opts, ctl, ex: Excluder::new(&opts.excludes)?, jobs: vec![], stats: Stats::default(), ignores: vec![], map: None };
    let mut roots = Vec::new();
    for (i, src) in opts.sources.iter().enumerate() {
        let key = src.to_string_lossy().to_string();
        let read = opts.read_from.get(i).cloned().flatten().unwrap_or_else(|| src.clone());
        w.map = (read != *src).then(|| (read.clone(), src.clone()));
        // A source that isn't there fails the backup: a snapshot without it would look like
        // everything in it had been deleted.
        let m = fs::metadata(&read).map_err(|e| Error::new(format!("{key} can't be read ({e}). Is its drive or share connected?")))?;
        let prev = prev_root.as_ref().and_then(|t| t.get(&key)).cloned();
        if m.is_dir() {
            if let Some(kept) = w.keep(src, prev.as_ref()) {
                let Entry::Kept(mut n) = kept else { unreachable!() };
                n.name = key;
                roots.push(Entry::Kept(n));
                continue;
            }
            let prev_tree = match prev.and_then(|n| n.subtree) {
                Some(id) => Some(repo.load_tree(&id)?),
                None => None,
            };
            roots.push(Entry::Dir(w.dir(key, &read, &m, prev_tree)?));
        } else if let Some(e) = w.file(key, &read, &m, prev.as_ref()) {
            roots.push(e);
        }
    }

    // 2. Read.
    ctl.progress.phase.store(PHASE_READ, Relaxed);
    let jobs = std::mem::take(&mut w.jobs);
    let results: Vec<Result<Vec<Id>>> = jobs
        .par_iter()
        .map(|job| {
            if ctl.cancel.load(Relaxed) {
                return Err(cancelled());
            }
            *ctl.progress.current.lock().unwrap() = job.shown.to_string_lossy().to_string();
            let r = read_file(repo, ctl, job);
            ctl.progress.files_read.fetch_add(1, Relaxed);
            r
        })
        .collect();
    ctl.checkpoint()?;
    let mut stats = w.stats;
    let mut read_bytes = 0;
    let mut contents: Vec<Option<Vec<Id>>> = Vec::with_capacity(results.len());
    for (job, r) in jobs.iter().zip(results) {
        match r {
            Ok(ids) => {
                read_bytes += job.size;
                contents.push(Some(ids));
            }
            Err(e) if e == cancelled() => return Err(e),
            Err(e) => {
                let kept = if job.previous.is_some() { "; the previous version is kept" } else { "" };
                stats.error_count += 1;
                if stats.errors.len() < 50 {
                    stats.errors.push(format!("{}: {e}{kept}", job.shown.display()));
                }
                contents.push(None);
            }
        }
    }

    // 3. Commit.
    ctl.progress.phase.store(PHASE_COMMIT, Relaxed);
    fn build(repo: &Repo, entries: Vec<Entry>, contents: &mut [Option<Vec<Id>>], jobs: &[Job]) -> Result<Tree> {
        let mut t = Tree::default();
        for e in entries {
            match e {
                Entry::Dir(d) => {
                    let sub = build(repo, d.entries, contents, jobs)?;
                    let mut n = d.node;
                    // A folder's node carries what is below it, so an unchanged one can be kept
                    // next time without opening it.
                    for c in &sub.nodes {
                        match c.kind {
                            NodeKind::File => {
                                n.files += 1;
                                n.size += c.size;
                            }
                            NodeKind::Dir => {
                                n.files += c.files;
                                n.size += c.size;
                            }
                            NodeKind::Symlink => {}
                        }
                    }
                    n.subtree = Some(repo.save_tree(&sub)?);
                    t.nodes.push(n);
                }
                Entry::File(mut n, None) => {
                    n.content.shrink_to_fit();
                    t.nodes.push(n)
                }
                Entry::File(mut n, Some(i)) => match contents[i].take() {
                    Some(ids) => {
                        n.content = ids;
                        t.nodes.push(n);
                    }
                    // Unreadable now: the last good version, if there is one.
                    None => {
                        if let Some(p) = jobs[i].previous.clone() {
                            t.nodes.push(p);
                        }
                    }
                },
                Entry::Link(n) | Entry::Kept(n) => t.nodes.push(n),
            }
        }
        t.sort();
        Ok(t)
    }
    let root = build(repo, roots, &mut contents, &jobs)?;
    let tree = repo.save_tree(&root)?;
    repo.flush()?;

    stats.read_bytes = read_bytes;
    stats.added_bytes = repo.counters.added_bytes.load(Relaxed) - before.0;
    stats.stored_bytes = repo.counters.stored_bytes.load(Relaxed) - before.1;
    stats.dup_bytes = repo.counters.dup_bytes.load(Relaxed) - before.2;
    stats.duration_ms = started.elapsed().as_millis() as u64;
    let mut snap = Snapshot {
        id: Id::default(),
        time: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        plan: opts.plan.clone(),
        host: crate::repo::hostname(),
        sources: opts.sources.iter().map(|s| s.to_string_lossy().to_string()).collect(),
        tree,
        parent: parent.map(|p| p.id),
        kind: if opts.full || parent.is_none() { "full".into() } else { "incremental".into() },
        stats,
    };
    repo.save_snapshot(&mut snap)?;
    Ok(snap)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::backend::Folder;
    use crate::crypto::Kdf;

    pub fn setup(pw: Option<&str>) -> (tempfile::TempDir, tempfile::TempDir, Arc<Repo>) {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let repo = Arc::new(Repo::init_with(Arc::new(Folder::new(dst.path())), pw, Kdf::fast()).unwrap().repo);
        (src, dst, repo)
    }

    pub fn opts(src: &Path) -> Options {
        Options { plan: "p".into(), sources: vec![src.to_path_buf()], ..Default::default() }
    }

    fn write(p: &Path, s: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn incremental_reads_only_what_changed() {
        let (src, _dst, repo) = setup(Some("pw"));
        write(&src.path().join("a.txt"), b"alpha");
        write(&src.path().join("sub/b.txt"), &vec![3u8; 3_000_000]);
        let ctl = Control::default();
        let s1 = run(&repo, &opts(src.path()), None, &ctl).unwrap();
        assert_eq!(s1.stats.files, 2);
        assert_eq!(s1.stats.new_files, 2);
        assert_eq!(s1.kind, "full");

        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&src.path().join("a.txt"), b"alpha, changed");
        let ctl = Control::default();
        let s2 = run(&repo, &opts(src.path()), Some(&s1), &ctl).unwrap();
        assert_eq!(s2.kind, "incremental");
        assert_eq!(ctl.progress.files_to_read.load(Relaxed), 1);
        assert_eq!(s2.stats.changed_files, 1);
        assert_eq!(s2.stats.read_bytes, 14);

        // A full backup reads everything but stores nothing new.
        let mut o = opts(src.path());
        o.full = true;
        let ctl = Control::default();
        let s3 = run(&repo, &o, Some(&s2), &ctl).unwrap();
        assert_eq!(ctl.progress.files_to_read.load(Relaxed), 2);
        assert_eq!(s3.stats.added_bytes, 0);
        assert_eq!(s3.tree, s2.tree);
        assert_eq!(repo.snapshots().unwrap().len(), 3);
    }

    #[test]
    fn exclusions_and_missing_sources() {
        let (src, _dst, repo) = setup(None);
        write(&src.path().join("keep.txt"), b"k");
        write(&src.path().join("x.tmp"), b"t");
        write(&src.path().join("node_modules/m.js"), b"m");
        write(&src.path().join("proj/.gitignore"), b"build/\n*.log\n");
        write(&src.path().join("proj/build/out.o"), b"o");
        write(&src.path().join("proj/run.log"), b"l");
        write(&src.path().join("proj/main.rs"), b"fn main(){}");
        let mut o = opts(src.path());
        o.excludes = vec!["*.tmp".into(), "node_modules/".into(), format!("{}/proj/main.rs", src.path().display())];
        o.gitignore = true;
        let s = run(&repo, &o, None, &Control::default()).unwrap();
        let root = repo.load_tree(&s.tree).unwrap();
        let top = repo.load_tree(&root.nodes[0].subtree.unwrap()).unwrap();
        let names: Vec<_> = top.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["keep.txt", "proj"]);
        let proj = repo.load_tree(&top.get("proj").unwrap().subtree.unwrap()).unwrap();
        let names: Vec<_> = proj.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec![".gitignore"]);

        let o = opts(&src.path().join("not-there"));
        let e = run(&repo, &o, None, &Control::default()).unwrap_err();
        assert!(e.0.contains("connected"), "{e}");
    }

    #[test]
    fn trusts_macos_about_unchanged_folders() {
        let (src, _dst, repo) = setup(None);
        write(&src.path().join("one/a"), b"a");
        write(&src.path().join("two/b"), b"b");
        let s1 = run(&repo, &opts(src.path()), None, &Control::default()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&src.path().join("one/a"), b"a, unreported");
        write(&src.path().join("two/b"), b"b, reported");
        let mut o = opts(src.path());
        o.changes = Some(Changes::from_events([src.path().join("two")], vec![]));
        let ctl = Control::default();
        let s2 = run(&repo, &o, Some(&s1), &ctl).unwrap();
        assert_eq!(ctl.progress.files_to_read.load(Relaxed), 1);
        assert_eq!(s2.stats.files, 2, "the kept folder's files still count");
        let root = repo.load_tree(&s2.tree).unwrap();
        let top = repo.load_tree(&root.nodes[0].subtree.unwrap()).unwrap();
        assert_eq!(top.get("one").unwrap().files, 1);
        assert_eq!(top.get("one"), repo.load_tree(&repo.load_tree(&s1.tree).unwrap().nodes[0].subtree.unwrap()).unwrap().get("one"));
    }

    #[test]
    fn reads_through_a_snapshot_mount_but_records_the_real_paths() {
        let (src, _dst, repo) = setup(None);
        let mnt = tempfile::tempdir().unwrap();
        write(&mnt.path().join("x.txt"), b"as it was");
        let mut o = opts(src.path());
        o.read_from = vec![Some(mnt.path().to_path_buf())];
        o.excludes = vec![format!("{}/skip", src.path().display())];
        write(&mnt.path().join("skip/y"), b"y");
        let s = run(&repo, &o, None, &Control::default()).unwrap();
        let root = repo.load_tree(&s.tree).unwrap();
        assert_eq!(root.nodes[0].name, src.path().to_string_lossy());
        let top = repo.load_tree(&root.nodes[0].subtree.unwrap()).unwrap();
        let names: Vec<_> = top.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["x.txt"]);
    }

    #[test]
    fn cancelling_leaves_no_snapshot() {
        let (src, _dst, repo) = setup(None);
        write(&src.path().join("a"), b"a");
        let ctl = Control::default();
        ctl.cancel.store(true, Relaxed);
        assert_eq!(run(&repo, &opts(src.path()), None, &ctl).unwrap_err(), cancelled());
        assert!(repo.snapshots().unwrap().is_empty());
    }
}
