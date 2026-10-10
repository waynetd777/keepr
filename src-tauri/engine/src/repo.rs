// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening a repository, and reading and writing its blobs, indexes, snapshots and locks.

use crate::backend::Backend;
use crate::crypto::{Encryption, Kdf, Keys};
use crate::tree::Tree;
use crate::{Error, Id, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

pub const CONFIG: &str = "keepr-repo.json";
pub const FORMAT: u32 = 1;
/// A pack is written once it holds this much.
pub const PACK_TARGET: usize = 16 << 20;
/// An index file is closed once it names this many blobs, and the next one started. One file per
/// backup (and prune's one for everything) would otherwise grow without limit: at about 100 bytes
/// a blob, a few million blobs make one file of hundreds of MB, read whole on every open.
pub const INDEX_ENTRIES: usize = 100_000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunker {
    pub min: u32,
    pub avg: u32,
    pub max: u32,
}

impl Default for Chunker {
    fn default() -> Chunker {
        Chunker { min: 256 << 10, avg: 1 << 20, max: 4 << 20 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub format: u32,
    pub id: Id,
    pub created: String,
    pub chunker: Chunker,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<Encryption>,
    /// Names blobs in an unencrypted repository (an encrypted one derives it from the master key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_key: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Kind {
    #[serde(rename = "d")]
    Data,
    #[serde(rename = "t")]
    Tree,
}

/// Where a blob is: which pack, and where in it.
#[derive(Clone, Copy, Debug)]
pub struct Loc {
    pub pack: u32,
    pub offset: u32,
    pub len: u32,
    pub raw: u32,
    pub kind: Kind,
}

/// One blob's entry in a pack's header and in an index file: [id, kind, offset, stored length, plain length].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry(pub Id, pub Kind, pub u32, pub u32, pub u32);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackRecord {
    pub pack: Id,
    pub blobs: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Default)]
struct IndexFile {
    packs: Vec<PackRecord>,
}

#[derive(Default)]
pub struct Index {
    pub packs: Vec<Id>,
    pack_no: HashMap<Id, u32>,
    pub blobs: HashMap<Id, Loc>,
    /// The index files this was read from, which prune replaces.
    pub files: Vec<String>,
}

impl Packer {
    fn holds(&self, id: &Id) -> bool {
        self.pending.contains(id) || self.inflight.contains(id)
    }
}

impl Index {
    pub fn add_record(&mut self, rec: &PackRecord) {
        let n = *self.pack_no.entry(rec.pack).or_insert_with(|| {
            self.packs.push(rec.pack);
            (self.packs.len() - 1) as u32
        });
        for Entry(id, kind, offset, len, raw) in &rec.blobs {
            self.blobs.insert(*id, Loc { pack: n, offset: *offset, len: *len, raw: *raw, kind: *kind });
        }
    }
}

pub fn pack_path(id: &Id) -> String {
    let h = id.hex();
    format!("packs/{}/{h}.pack", &h[..2])
}

struct Packer {
    buf: Vec<u8>,
    entries: Vec<Entry>,
    /// Blobs in the pack being filled, so a chunk seen twice in one backup is stored once.
    pending: HashSet<Id>,
    /// Blobs in a full pack on its way to the backend: out of `pending` but not yet in the index.
    /// Without this a chunk seen again during the upload would be in neither, and stored twice.
    inflight: HashSet<Id>,
}

/// Counts for the backup's progress and its snapshot's statistics.
#[derive(Default)]
pub struct Counters {
    pub added_bytes: std::sync::atomic::AtomicU64,
    pub stored_bytes: std::sync::atomic::AtomicU64,
    pub dup_bytes: std::sync::atomic::AtomicU64,
}

pub struct Repo {
    pub backend: Arc<dyn Backend>,
    pub config: Config,
    keys: Keys,
    pub index: RwLock<Index>,
    packer: Mutex<Packer>,
    /// Packs written by this session and not yet in an index file.
    written: Mutex<Vec<PackRecord>>,
    /// Why a pack couldn't be written, since the last flush. Other threads have by then been told
    /// their blobs in that pack were stored, so the flush must fail and no snapshot be saved.
    failed: Mutex<Option<Error>>,
    trees: Mutex<HashMap<Id, Arc<Tree>>>,
    pub counters: Counters,
}

/// How to open an encrypted repository.
pub enum Secret<'a> {
    None,
    Password(&'a str),
    RecoveryKey(&'a str),
}

pub struct Created {
    pub repo: Repo,
    /// For an encrypted repository: the recovery key, to show the user once.
    pub recovery_key: Option<String>,
}

impl Repo {
    /// Makes a new repository. Refuses if one is already there: two plans must never share one by accident.
    pub fn init(backend: Arc<dyn Backend>, password: Option<&str>) -> Result<Created> {
        Self::init_with(backend, password, Kdf::new())
    }

    pub fn init_with(backend: Arc<dyn Backend>, password: Option<&str>, kdf: Kdf) -> Result<Created> {
        if backend.exists(CONFIG) {
            return Err(Error::new(format!("There is already a backup at {}.", backend.describe())));
        }
        let (encryption, keys, id_key, recovery) = match password {
            Some(p) => {
                let (e, master, rec) = Encryption::create(p, kdf)?;
                (Some(e), Keys::from_master(&master), None, Some(rec))
            }
            None => {
                let k = Id::random();
                (None, Keys::plain(k.0), Some(k.hex()), None)
            }
        };
        let config = Config {
            format: FORMAT,
            id: Id::random(),
            created: chrono::Utc::now().to_rfc3339(),
            chunker: Chunker::default(),
            encryption,
            id_key,
        };
        backend.write(CONFIG, &serde_json::to_vec_pretty(&config)?)?;
        Ok(Created { repo: Repo::with(backend, config, keys), recovery_key: recovery })
    }

    fn with(backend: Arc<dyn Backend>, config: Config, keys: Keys) -> Repo {
        Repo {
            backend,
            config,
            keys,
            index: RwLock::new(Index::default()),
            packer: Mutex::new(Packer { buf: Vec::new(), entries: Vec::new(), pending: HashSet::new(), inflight: HashSet::new() }),
            written: Mutex::new(Vec::new()),
            failed: Mutex::new(None),
            trees: Mutex::new(HashMap::new()),
            counters: Counters::default(),
        }
    }

    pub fn read_config(backend: &dyn Backend) -> Result<Config> {
        let raw = match backend.read(CONFIG) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::new(format!("There's no Keepr backup at {}.", backend.describe())))
            }
            Err(e) => return Err(Error::new(format!("Can't read the backup at {}: {e}", backend.describe()))),
        };
        let config: Config = serde_json::from_slice(&raw)?;
        if config.format > FORMAT {
            return Err(Error::new("This backup was made by a newer Keepr. Update Keepr to use it."));
        }
        Ok(config)
    }

    /// Opens a repository and reads its index.
    pub fn open(backend: Arc<dyn Backend>, secret: Secret) -> Result<Repo> {
        let config = Self::read_config(backend.as_ref())?;
        let keys = match (&config.encryption, secret) {
            (None, _) => {
                let k = hex::decode(config.id_key.as_deref().unwrap_or("")).ok().and_then(|v| <[u8; 32]>::try_from(v).ok());
                Keys::plain(k.ok_or_else(|| Error::new("damaged repository settings: no id key"))?)
            }
            (Some(_), Secret::None) => return Err(Error::new("This backup is encrypted. Its password is needed.")),
            (Some(e), Secret::Password(p)) => Keys::from_master(&e.open_with_password(p)?),
            (Some(e), Secret::RecoveryKey(k)) => Keys::from_master(&e.open_with_recovery(k)?),
        };
        let repo = Repo::with(backend, config, keys);
        repo.load_index()?;
        Ok(repo)
    }

    pub fn encrypted(&self) -> bool {
        self.keys.encrypted()
    }

    /// Seals the master key with a new password. Needs the repository open (with the old password
    /// or the recovery key), and is the one place the settings file is rewritten.
    pub fn change_password(&mut self, current: Secret, new: &str) -> Result<()> {
        let e = self.config.encryption.clone().ok_or_else(|| Error::new("This backup isn't encrypted."))?;
        let master = match current {
            Secret::Password(p) => e.open_with_password(p)?,
            Secret::RecoveryKey(k) => e.open_with_recovery(k)?,
            Secret::None => return Err(Error::new("The current password or the recovery key is needed.")),
        };
        self.config.encryption = Some(e.with_new_password(&master, new)?);
        self.backend.write(CONFIG, &serde_json::to_vec_pretty(&self.config)?)?;
        Ok(())
    }

    // ---- sealed documents (index files, snapshots): zstd, then sealed ----

    pub fn encode(&self, plain: &[u8]) -> Vec<u8> {
        // A flag byte says whether the rest is compressed: already-compressed data (photos,
        // video, zip) usually comes out bigger, and is kept as it was.
        let z = zstd::bulk::compress(plain, 3).unwrap_or_default();
        let mut body = Vec::with_capacity(plain.len().min(z.len()) + 1);
        if !z.is_empty() && z.len() < plain.len() {
            body.push(1);
            body.extend_from_slice(&z);
        } else {
            body.push(0);
            body.extend_from_slice(plain);
        }
        self.keys.seal(&body)
    }

    pub fn decode(&self, stored: &[u8], raw_len: Option<usize>) -> Result<Vec<u8>> {
        let body = self.keys.open(stored)?;
        match body.first() {
            Some(0) => Ok(body[1..].to_vec()),
            // A blob's plain length is in the index, and bounds the output. An index file or a
            // snapshot has no such bound, and may be any size, so it is decompressed as a stream.
            Some(1) => match raw_len {
                Some(n) => zstd::bulk::decompress(&body[1..], n.max(1)),
                None => zstd::stream::decode_all(&body[1..]),
            }
            .map_err(|e| Error::new(format!("damaged data: {e}"))),
            _ => Err(Error::new("damaged data: unknown encoding")),
        }
    }

    fn write_doc(&self, path: &str, v: &impl Serialize) -> Result<()> {
        let raw = serde_json::to_vec(v)?;
        self.backend.write(path, &self.encode(&raw))?;
        Ok(())
    }

    fn read_doc<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let stored = self.backend.read(path)?;
        let plain = self.decode(&stored, None)?;
        Ok(serde_json::from_slice(&plain)?)
    }

    // ---- index ----

    /// Reads the index afresh from the backend, replacing the one in memory. Packs this session
    /// wrote and hasn't yet named in an index file are kept in it: they are on disk and in use.
    pub fn load_index(&self) -> Result<()> {
        let mut ix = Index::default();
        for f in self.backend.list("index")? {
            if !f.ends_with(".idx") {
                continue;
            }
            let path = format!("index/{f}");
            let doc: IndexFile = self.read_doc(&path).map_err(|e| Error::new(format!("{path}: {e}")))?;
            for rec in &doc.packs {
                ix.add_record(rec);
            }
            ix.files.push(path);
        }
        for rec in self.written.lock().unwrap().iter() {
            ix.add_record(rec);
        }
        *self.index.write().unwrap() = ix;
        Ok(())
    }

    /// Writes index files naming these packs, a new file every `INDEX_ENTRIES` blobs or so (a
    /// pack's record is never split). Returns their paths.
    pub fn write_index(&self, packs: Vec<PackRecord>) -> Result<Vec<String>> {
        let mut paths = Vec::new();
        let mut doc = IndexFile::default();
        let mut n = 0;
        let mut it = packs.into_iter().peekable();
        while let Some(rec) = it.next() {
            n += rec.blobs.len();
            doc.packs.push(rec);
            if n >= INDEX_ENTRIES || it.peek().is_none() {
                let path = format!("index/{}.idx", Id::random().hex());
                self.write_doc(&path, &std::mem::take(&mut doc))?;
                paths.push(path);
                n = 0;
            }
        }
        Ok(paths)
    }

    pub fn has(&self, id: &Id) -> bool {
        self.index.read().unwrap().blobs.contains_key(id)
    }

    // ---- writing blobs ----

    pub fn id_of(&self, plain: &[u8]) -> Id {
        self.keys.id(plain)
    }

    /// Stores a blob unless it is already stored. Returns its id and whether it was new.
    pub fn add(&self, kind: Kind, plain: &[u8]) -> Result<(Id, bool)> {
        use std::sync::atomic::Ordering::Relaxed;
        let id = self.keys.id(plain);
        if self.has(&id) || self.packer.lock().unwrap().holds(&id) {
            self.counters.dup_bytes.fetch_add(plain.len() as u64, Relaxed);
            return Ok((id, false));
        }
        let stored = self.encode(plain);
        let full = {
            let mut p = self.packer.lock().unwrap();
            // Asked again under the lock: a pack written meanwhile moves its blobs into the index
            // before out of `inflight`, so one of the two has this blob if it is stored.
            if p.holds(&id) || self.has(&id) || !p.pending.insert(id) {
                // Another thread stored the same chunk while this one was compressing it.
                self.counters.dup_bytes.fetch_add(plain.len() as u64, Relaxed);
                return Ok((id, false));
            }
            let offset = p.buf.len() as u32;
            p.buf.extend_from_slice(&stored);
            p.entries.push(Entry(id, kind, offset, stored.len() as u32, plain.len() as u32));
            if p.buf.len() >= PACK_TARGET {
                Some(Self::take(&mut p))
            } else {
                None
            }
        };
        self.counters.added_bytes.fetch_add(plain.len() as u64, Relaxed);
        self.counters.stored_bytes.fetch_add(stored.len() as u64, Relaxed);
        if let Some((buf, entries)) = full {
            self.write_pack(buf, entries)?;
        }
        Ok((id, true))
    }

    /// The pack being filled, to write; its blobs move from `pending` to `inflight`.
    fn take(p: &mut Packer) -> (Vec<u8>, Vec<Entry>) {
        let ids = std::mem::take(&mut p.pending);
        p.inflight.extend(ids);
        (std::mem::take(&mut p.buf), std::mem::take(&mut p.entries))
    }

    /// Ends a pack's blobs (`buf`, which `entries` describe) with the pack's header and its length,
    /// as every pack is laid out.
    pub fn pack_bytes(&self, buf: &mut Vec<u8>, entries: &[Entry]) -> Result<()> {
        let header = self.encode(&serde_json::to_vec(entries)?);
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
        Ok(())
    }

    /// Packs are named by a random id: the name says nothing about what is inside.
    fn write_pack(&self, mut buf: Vec<u8>, entries: Vec<Entry>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let res = (|| -> Result<Id> {
            self.pack_bytes(&mut buf, &entries)?;
            let pack = Id::random();
            self.backend
                .write(&pack_path(&pack), &buf)
                .map_err(|e| Error::new(format!("Couldn't write to {}: {e}", self.backend.describe())))?;
            Ok(pack)
        })();
        let pack = match res {
            Ok(p) => p,
            Err(e) => {
                // The other blobs in this pack were reported stored to whoever added them, and
                // their files' nodes will name them: the backup can't be saved now.
                let mut p = self.packer.lock().unwrap();
                for Entry(id, ..) in &entries {
                    p.inflight.remove(id);
                }
                self.failed.lock().unwrap().get_or_insert_with(|| e.clone());
                return Err(e);
            }
        };
        let rec = PackRecord { pack, blobs: entries };
        // Into the in-memory index at once, and only then out of `inflight`, so a blob is always
        // findable in one or the other.
        self.index.write().unwrap().add_record(&rec);
        {
            let mut p = self.packer.lock().unwrap();
            for Entry(id, ..) in &rec.blobs {
                p.inflight.remove(id);
            }
        }
        self.written.lock().unwrap().push(rec);
        Ok(())
    }

    /// Writes the pack being filled, then index files naming every pack this session wrote.
    /// After this, a snapshot may refer to any blob added so far. Fails if any pack since the
    /// last flush couldn't be written, as then some blob added since isn't stored.
    pub fn flush(&self) -> Result<()> {
        let (buf, entries) = Self::take(&mut self.packer.lock().unwrap());
        let last = self.write_pack(buf, entries);
        // The packs that were written are named all the same, so they aren't taken for leftovers.
        let packs = std::mem::take(&mut *self.written.lock().unwrap());
        if !packs.is_empty() {
            let paths = self.write_index(packs.clone()).inspect_err(|_| self.written.lock().unwrap().extend(packs))?;
            self.index.write().unwrap().files.extend(paths);
        }
        if let Some(e) = self.failed.lock().unwrap().take() {
            return Err(e);
        }
        last
    }

    // ---- reading blobs ----

    pub fn load(&self, id: &Id) -> Result<Vec<u8>> {
        let (loc, pack) = {
            let ix = self.index.read().unwrap();
            let loc = *ix.blobs.get(id).ok_or_else(|| Error::new(format!("missing data {}", id.short())))?;
            (loc, ix.packs[loc.pack as usize])
        };
        let stored = self
            .backend
            .read_at(&pack_path(&pack), loc.offset as u64, loc.len as u64)
            .map_err(|e| Error::new(format!("Can't read {}: {e}", pack_path(&pack))))?;
        let plain = self.decode(&stored, Some(loc.raw as usize))?;
        if self.keys.id(&plain) != *id {
            return Err(Error::new(format!("damaged data {}: it doesn't match its id", id.short())));
        }
        Ok(plain)
    }

    /// The raw stored bytes of a blob, for prune to copy between packs without decrypting.
    pub fn load_stored(&self, pack: &Id, loc: &Loc) -> Result<Vec<u8>> {
        Ok(self.backend.read_at(&pack_path(pack), loc.offset as u64, loc.len as u64)?)
    }

    pub fn load_tree(&self, id: &Id) -> Result<Arc<Tree>> {
        if let Some(t) = self.trees.lock().unwrap().get(id) {
            return Ok(t.clone());
        }
        let t: Tree = serde_json::from_slice(&self.load(id)?)?;
        let t = Arc::new(t);
        let mut cache = self.trees.lock().unwrap();
        if cache.len() > 20_000 {
            cache.clear();
        }
        cache.insert(*id, t.clone());
        Ok(t)
    }

    pub fn save_tree(&self, t: &Tree) -> Result<Id> {
        let raw = serde_json::to_vec(t)?;
        Ok(self.add(Kind::Tree, &raw)?.0)
    }

    // ---- snapshots ----

    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        let mut out = Vec::new();
        for f in self.backend.list("snapshots")? {
            let Some(hexid) = f.strip_suffix(".snap") else { continue };
            let Ok(id) = Id::parse(hexid) else { continue };
            let mut s: Snapshot = self.read_doc(&format!("snapshots/{f}"))?;
            s.id = id;
            out.push(s);
        }
        out.sort_by(|a, b| a.time.cmp(&b.time));
        Ok(out)
    }

    pub fn save_snapshot(&self, s: &mut Snapshot) -> Result<()> {
        s.id = Id::random();
        self.write_doc(&format!("snapshots/{}.snap", s.id.hex()), s)
    }

    pub fn forget(&self, id: &Id) -> Result<()> {
        Ok(self.backend.remove(&format!("snapshots/{}.snap", id.hex()))?)
    }

    // ---- locks ----

    /// Records that this process is using the repository. Prune takes an exclusive lock and
    /// refuses while anyone else holds one; a lock whose process has gone is cleared.
    pub fn lock(self: &Arc<Self>, exclusive: bool) -> Result<Lock> {
        self.lock_conflict(exclusive, None)?;
        let path = format!("locks/{}.lock", Id::random().hex());
        let l = LockFile { host: hostname(), pid: std::process::id(), time: chrono::Utc::now().to_rfc3339(), exclusive };
        self.backend.write(&path, &serde_json::to_vec(&l)?)?;
        // Looking and then writing isn't one step: another Mac may have done the same meanwhile.
        // Look again now this lock is there for it to see, and back off if one got in.
        if let Err(e) = self.lock_conflict(exclusive, Some(&path)) {
            let _ = self.backend.remove(&path);
            return Err(e);
        }
        let (stop, rx) = std::sync::mpsc::channel::<()>();
        let (backend, p) = (self.backend.clone(), path.clone());
        // A lock from another Mac counts as stale a day after its time, so a job running longer
        // than that keeps its time fresh.
        let refresh = std::thread::spawn(move || {
            let mut l = l;
            while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(LOCK_REFRESH) {
                l.time = chrono::Utc::now().to_rfc3339();
                if let Ok(raw) = serde_json::to_vec(&l) {
                    let _ = backend.write(&p, &raw);
                }
            }
        });
        Ok(Lock { repo: self.clone(), path, stop: Some(stop), refresh: Some(refresh) })
    }

    /// Fails if a lock other than `mine` stops this one being taken. Clears stale ones: on this
    /// Mac, a lock whose process has gone; from another Mac, one not refreshed for a day (its
    /// process can't be asked).
    fn lock_conflict(&self, exclusive: bool, mine: Option<&str>) -> Result<()> {
        let host = hostname();
        let me = std::process::id();
        for f in self.backend.list("locks")? {
            let path = format!("locks/{f}");
            if mine == Some(path.as_str()) {
                continue;
            }
            let Ok(raw) = self.backend.read(&path) else { continue };
            let Ok(l) = serde_json::from_slice::<LockFile>(&raw) else { continue };
            let stale = if l.host == host {
                !pid_alive(l.pid)
            } else {
                chrono::DateTime::parse_from_rfc3339(&l.time).is_ok_and(|t| chrono::Utc::now().signed_duration_since(t).num_hours() >= 24)
            };
            if stale {
                let _ = self.backend.remove(&path);
                continue;
            }
            if (exclusive || l.exclusive) && !(l.host == host && l.pid == me) {
                return Err(Error::new(format!(
                    "The backup is in use by {} (since {}).",
                    if l.host == host { "another Keepr on this Mac".to_string() } else { l.host.clone() },
                    l.time
                )));
            }
        }
        Ok(())
    }
}

/// How often a held lock's time is rewritten.
const LOCK_REFRESH: std::time::Duration = std::time::Duration::from_secs(3600);

#[derive(Serialize, Deserialize)]
struct LockFile {
    host: String,
    pid: u32,
    time: String,
    exclusive: bool,
}

pub struct Lock {
    repo: Arc<Repo>,
    path: String,
    /// Dropped to stop the thread that keeps the lock's time fresh.
    stop: Option<std::sync::mpsc::Sender<()>>,
    refresh: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Lock {
    fn drop(&mut self) {
        // The refresher first, so it can't write the lock back after it is removed.
        drop(self.stop.take());
        if let Some(t) = self.refresh.take() {
            let _ = t.join();
        }
        let _ = self.repo.backend.remove(&self.path);
    }
}

pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if r != 0 {
        return "this Mac".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).trim_end_matches(".local").to_string()
}

fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 || *libc::__error() == libc::EPERM }
}

/// One backup: when it was taken and the tree of everything in it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    #[serde(skip)]
    pub id: Id,
    pub time: String,
    pub plan: String,
    pub host: String,
    /// The folders backed up, as absolute paths; the root tree has one entry per source, named by its path.
    pub sources: Vec<String>,
    pub tree: Id,
    #[serde(default)]
    pub parent: Option<Id>,
    /// "incremental" (unchanged files weren't re-read) or "full" (every file was).
    pub kind: String,
    pub stats: Stats,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Stats {
    pub files: u64,
    pub dirs: u64,
    /// Everything in the snapshot, as the files' sizes add up.
    pub bytes: u64,
    pub new_files: u64,
    pub changed_files: u64,
    /// Files in the previous snapshot that aren't in this one.
    #[serde(default)]
    pub removed_files: u64,
    pub read_bytes: u64,
    /// New data, before compression.
    pub added_bytes: u64,
    /// What that took in the repository, after compression and encryption.
    pub stored_bytes: u64,
    /// Data read that was already stored.
    pub dup_bytes: u64,
    pub error_count: u64,
    /// The first few problems, for people to read.
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Folder;

    fn repo(dir: &std::path::Path, pw: Option<&str>) -> Repo {
        Repo::init_with(Arc::new(Folder::new(dir)), pw, Kdf::fast()).unwrap().repo
    }

    #[test]
    fn blobs_dedupe_and_survive_reopening() {
        let t = tempfile::tempdir().unwrap();
        let r = repo(t.path(), Some("pw"));
        let big = vec![7u8; 100_000];
        let (a, new) = r.add(Kind::Data, &big).unwrap();
        assert!(new);
        let (b, new) = r.add(Kind::Data, &big).unwrap();
        assert!(!new);
        assert_eq!(a, b);
        r.flush().unwrap();
        let r2 = Repo::open(Arc::new(Folder::new(t.path())), Secret::Password("pw")).unwrap();
        assert_eq!(r2.load(&a).unwrap(), big);
        assert!(Repo::open(Arc::new(Folder::new(t.path())), Secret::Password("nope")).is_err());
        assert!(Repo::open(Arc::new(Folder::new(t.path())), Secret::None).is_err());
        // Compressible data is stored smaller.
        let stored = r2.index.read().unwrap().blobs[&a].len;
        assert!(stored < 1000);
    }

    #[test]
    fn refuses_to_init_twice_and_reports_missing() {
        let t = tempfile::tempdir().unwrap();
        repo(t.path(), None);
        assert!(Repo::init(Arc::new(Folder::new(t.path())), None).is_err());
        let e = Repo::open(Arc::new(Folder::new(t.path().join("gone"))), Secret::None).err().unwrap();
        assert!(e.0.contains("no Keepr backup"));
    }

    #[test]
    fn exclusive_lock_waits_for_others() {
        let t = tempfile::tempdir().unwrap();
        let r = Arc::new(repo(t.path(), None));
        // A lock from another host that is still fresh blocks prune.
        let other = LockFile { host: "elsewhere".into(), pid: 1, time: chrono::Utc::now().to_rfc3339(), exclusive: false };
        r.backend.write("locks/other.lock", &serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(r.lock(true).is_err());
        let shared = r.lock(false).unwrap();
        drop(shared);
        r.backend.remove("locks/other.lock").unwrap();
        let _ex = r.lock(true).unwrap();
    }

    #[test]
    fn big_index_files_read_back_and_are_split() {
        let t = tempfile::tempdir().unwrap();
        let r = repo(t.path(), Some("pw"));
        // A document past any fixed cap on its plain size still decodes.
        let big = vec![b'x'; 70 << 20];
        assert_eq!(r.decode(&r.encode(&big), None).unwrap().len(), big.len());
        // More blobs than one index file takes: two files, and all of it read back.
        let rec = |n: usize| PackRecord { pack: Id::random(), blobs: (0..n).map(|_| Entry(Id::random(), Kind::Data, 0, 1, 1)).collect() };
        let paths = r.write_index(vec![rec(INDEX_ENTRIES - 1), rec(2), rec(5)]).unwrap();
        assert_eq!(paths.len(), 2);
        r.load_index().unwrap();
        assert_eq!(r.index.read().unwrap().blobs.len(), INDEX_ENTRIES + 6);
    }

    /// A folder whose pack writes fail while `fail` is set.
    struct Flaky(Folder, std::sync::atomic::AtomicBool);

    impl Backend for Flaky {
        fn read(&self, p: &str) -> std::io::Result<Vec<u8>> {
            self.0.read(p)
        }
        fn read_at(&self, p: &str, o: u64, l: u64) -> std::io::Result<Vec<u8>> {
            self.0.read_at(p, o, l)
        }
        fn write(&self, p: &str, d: &[u8]) -> std::io::Result<()> {
            if p.starts_with("packs/") && self.1.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(std::io::Error::other("disk full"));
            }
            self.0.write(p, d)
        }
        fn list(&self, d: &str) -> std::io::Result<Vec<String>> {
            self.0.list(d)
        }
        fn remove(&self, p: &str) -> std::io::Result<()> {
            self.0.remove(p)
        }
        fn exists(&self, p: &str) -> bool {
            self.0.exists(p)
        }
        fn size(&self, p: &str) -> std::io::Result<u64> {
            self.0.size(p)
        }
        fn describe(&self) -> String {
            self.0.describe()
        }
    }

    #[test]
    fn a_failed_pack_fails_the_flush() {
        let t = tempfile::tempdir().unwrap();
        let b = Arc::new(Flaky(Folder::new(t.path()), false.into()));
        let r = Repo::init_with(b.clone(), None, Kdf::fast()).unwrap().repo;
        // One blob is reported stored, then its pack fails to write with the next.
        let noise = |seed: u64| {
            let mut x = seed;
            (0..PACK_TARGET / 2 + 1)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect::<Vec<u8>>()
        };
        let (a, _) = r.add(Kind::Data, &noise(1)).unwrap();
        b.1.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(r.add(Kind::Data, &noise(2)).is_err());
        b.1.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(r.flush().is_err(), "a snapshot must not be saved naming {}", a.short());
        assert!(!r.has(&a));
        // The failure is reported once; the next backup starts clean.
        r.add(Kind::Data, b"later").unwrap();
        r.flush().unwrap();
    }

    #[test]
    fn a_lock_on_this_mac_is_stale_only_when_its_process_has_gone() {
        let t = tempfile::tempdir().unwrap();
        let r = Arc::new(repo(t.path(), None));
        let old = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
        // Process 1 is always running: two days old, its lock still holds.
        let live = LockFile { host: hostname(), pid: 1, time: old.clone(), exclusive: false };
        r.backend.write("locks/live.lock", &serde_json::to_vec(&live).unwrap()).unwrap();
        assert!(r.lock(true).is_err());
        r.backend.remove("locks/live.lock").unwrap();
        let gone = LockFile { host: hostname(), pid: 999_999, time: chrono::Utc::now().to_rfc3339(), exclusive: true };
        r.backend.write("locks/gone.lock", &serde_json::to_vec(&gone).unwrap()).unwrap();
        let l = r.lock(true).unwrap();
        assert!(!r.backend.exists("locks/gone.lock"));
        drop(l);
        assert!(r.backend.list("locks").unwrap().is_empty());
    }

    #[test]
    fn password_changes() {
        let t = tempfile::tempdir().unwrap();
        let c = Repo::init_with(Arc::new(Folder::new(t.path())), Some("old"), Kdf::fast()).unwrap();
        let rec = c.recovery_key.unwrap();
        let mut r = c.repo;
        r.change_password(Secret::RecoveryKey(&rec), "new").unwrap();
        assert!(Repo::open(Arc::new(Folder::new(t.path())), Secret::Password("new")).is_ok());
        assert!(Repo::open(Arc::new(Folder::new(t.path())), Secret::Password("old")).is_err());
    }
}
