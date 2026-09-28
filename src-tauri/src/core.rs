//! The app's state and the job runner: one backup, restore, check or tidy-up at a time, from a
//! queue, on its own thread. The scheduler (scheduler.rs) and the window both add jobs; progress
//! and changes go to the window and the menu bar as events.

use crate::config::{self, Config, Every, Often, Place, Plan, Run, State};
use crate::places::{self, Mounts};
use crate::keychain;
use keepr_engine::backend::{Backend, Folder};
use keepr_engine::backup::{Control, Options, PHASE_COMMIT, PHASE_LOOK, PHASE_READ};
use keepr_engine::repo::{Repo, Secret, Snapshot, CONFIG};
use keepr_engine::restore::{Conflict, Target};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Job {
    Backup { plan: String, full: bool },
    Restore { plan: String, snapshot: String, items: Vec<String>, target: Target, conflict: Conflict },
    Check { plan: String, all: bool },
    Prune { plan: String },
}

impl Job {
    pub fn plan(&self) -> &str {
        match self {
            Job::Backup { plan, .. } | Job::Restore { plan, .. } | Job::Check { plan, .. } | Job::Prune { plan } => plan,
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Job::Backup { full: false, .. } => "backup",
            Job::Backup { full: true, .. } => "full",
            Job::Restore { .. } => "restore",
            Job::Check { .. } => "check",
            Job::Prune { .. } => "prune",
        }
    }
}

struct Current {
    id: String,
    job: Job,
    ctl: Arc<Control>,
    started_at: String,
    /// What's happening, beyond the engine's phase ("Tidying up", "Checking").
    stage: Mutex<String>,
    repo: Mutex<Option<Arc<Repo>>>,
    /// The repository's counters when the job started, so progress counts this job only.
    base: Mutex<(u64, u64)>,
    rate: Mutex<(Instant, u64, f64)>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub id: String,
    pub kind: String,
    pub plan: String,
    pub plan_name: String,
    /// "Looking", "Reading", "Sending", "Saving", "Tidying up", "Checking", "Restoring", "Connecting".
    pub stage: String,
    pub files_seen: u64,
    pub files_to_read: u64,
    pub files_read: u64,
    pub bytes_to_read: u64,
    pub bytes_read: u64,
    pub sent_bytes: u64,
    pub dup_bytes: u64,
    pub current: String,
    pub started_at: String,
    pub paused: bool,
    /// Bytes read per second, smoothed.
    pub rate: f64,
    pub eta_secs: Option<u64>,
    pub queued: usize,
}

pub struct Core {
    pub dir: PathBuf,
    pub config: Mutex<Config>,
    pub state: Mutex<State>,
    pub mounts: Mounts,
    repos: Mutex<HashMap<String, (PathBuf, Arc<Repo>)>>,
    queue: Mutex<VecDeque<(String, Job)>>,
    wake: Condvar,
    current: Mutex<Option<Arc<Current>>>,
    /// To the window and the menu bar: ("job", status) while something runs, ("changed", null) after.
    pub emit: Box<dyn Fn(&str, serde_json::Value) + Send + Sync>,
    pub notify: Box<dyn Fn(&str, &str) + Send + Sync>,
    /// Screenshot mode: nothing is saved or run.
    pub frozen: bool,
}

fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

pub fn human_bytes(b: u64) -> String {
    let (v, u) = match b {
        b if b >= 1 << 40 => (b as f64 / (1u64 << 40) as f64, "TB"),
        b if b >= 1 << 30 => (b as f64 / (1u64 << 30) as f64, "GB"),
        b if b >= 1 << 20 => (b as f64 / (1u64 << 20) as f64, "MB"),
        b if b >= 1 << 10 => (b as f64 / 1024.0, "KB"),
        b => return format!("{b} bytes"),
    };
    if v >= 100.0 { format!("{v:.0} {u}") } else { format!("{v:.1} {u}") }
}

/// A backend that keeps writes under a speed limit.
struct Throttle {
    inner: Folder,
    per_sec: u64,
    sent: Mutex<(Instant, u64)>,
}

impl Backend for Throttle {
    fn read(&self, p: &str) -> std::io::Result<Vec<u8>> {
        self.inner.read(p)
    }
    fn read_at(&self, p: &str, o: u64, l: u64) -> std::io::Result<Vec<u8>> {
        self.inner.read_at(p, o, l)
    }
    fn write(&self, p: &str, d: &[u8]) -> std::io::Result<()> {
        let wait = {
            let mut s = self.sent.lock().unwrap();
            if s.0.elapsed() > Duration::from_secs(10) {
                *s = (Instant::now(), 0);
            }
            s.1 += d.len() as u64;
            let should = Duration::from_secs_f64(s.1 as f64 / self.per_sec as f64);
            should.saturating_sub(s.0.elapsed())
        };
        std::thread::sleep(wait);
        self.inner.write(p, d)
    }
    fn list(&self, d: &str) -> std::io::Result<Vec<String>> {
        self.inner.list(d)
    }
    fn remove(&self, p: &str) -> std::io::Result<()> {
        self.inner.remove(p)
    }
    fn exists(&self, p: &str) -> bool {
        self.inner.exists(p)
    }
    fn size(&self, p: &str) -> std::io::Result<u64> {
        self.inner.size(p)
    }
    fn describe(&self) -> String {
        self.inner.describe()
    }
}

impl Core {
    pub fn new(dir: PathBuf, emit: Box<dyn Fn(&str, serde_json::Value) + Send + Sync>, notify: Box<dyn Fn(&str, &str) + Send + Sync>, frozen: bool) -> Arc<Core> {
        let config: Config = config::read(&dir, "config.json");
        let state: State = config::read(&dir, "state.json");
        Arc::new(Core {
            dir,
            config: Mutex::new(config),
            state: Mutex::new(state),
            mounts: Mounts::default(),
            repos: Mutex::new(HashMap::new()),
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            current: Mutex::new(None),
            emit,
            notify,
            frozen,
        })
    }

    pub fn save_config(&self) -> Result<(), String> {
        if self.frozen {
            return Ok(());
        }
        config::write(&self.dir, "config.json", &*self.config.lock().unwrap())
    }

    pub fn save_state(&self) {
        if self.frozen {
            return;
        }
        if let Err(e) = config::write(&self.dir, "state.json", &*self.state.lock().unwrap()) {
            eprintln!("state.json: {e}");
        }
    }

    pub fn changed(&self) {
        (self.emit)("changed", serde_json::Value::Null);
    }

    pub fn forget_repo(&self, plan: &str) {
        self.repos.lock().unwrap().remove(plan);
    }

    // ---- the queue ----

    /// Adds a job unless the same one is queued or running. Returns its id.
    pub fn enqueue(&self, job: Job) -> String {
        if self.frozen {
            return String::new();
        }
        if let Some(c) = self.current.lock().unwrap().as_ref() {
            if c.job == job {
                return c.id.clone();
            }
        }
        let mut q = self.queue.lock().unwrap();
        if let Some((id, _)) = q.iter().find(|(_, j)| *j == job) {
            return id.clone();
        }
        let id = config::new_id();
        q.push_back((id.clone(), job));
        drop(q);
        self.wake.notify_all();
        self.changed();
        id
    }

    pub fn busy_with(&self, plan: &str) -> bool {
        self.current.lock().unwrap().as_ref().is_some_and(|c| c.job.plan() == plan) || self.queue.lock().unwrap().iter().any(|(_, j)| j.plan() == plan)
    }

    pub fn running(&self) -> bool {
        self.current.lock().unwrap().is_some()
    }

    pub fn cancel(&self, id: &str) {
        self.queue.lock().unwrap().retain(|(i, _)| i != id);
        if let Some(c) = self.current.lock().unwrap().as_ref() {
            if c.id == id || id.is_empty() {
                c.ctl.cancel.store(true, Relaxed);
                c.ctl.pause.store(false, Relaxed);
            }
        }
        self.changed();
    }

    pub fn pause(&self, on: bool) {
        if let Some(c) = self.current.lock().unwrap().as_ref() {
            c.ctl.pause.store(on, Relaxed);
        }
        self.changed();
    }

    pub fn status(&self) -> Option<JobStatus> {
        let c = self.current.lock().unwrap().clone()?;
        let p = &c.ctl.progress;
        let plan_name = self.config.lock().unwrap().plan(c.job.plan()).map(|p| p.name.clone()).unwrap_or_default();
        let (sent, dup) = match c.repo.lock().unwrap().as_ref() {
            Some(r) => {
                let base = *c.base.lock().unwrap();
                (r.counters.stored_bytes.load(Relaxed).saturating_sub(base.0), r.counters.dup_bytes.load(Relaxed).saturating_sub(base.1))
            }
            None => (0, 0),
        };
        let read = p.bytes_read.load(Relaxed);
        let rate = {
            let mut r = c.rate.lock().unwrap();
            let dt = r.0.elapsed().as_secs_f64();
            if dt >= 1.0 {
                let inst = (read.saturating_sub(r.1)) as f64 / dt;
                r.2 = if r.2 == 0.0 { inst } else { r.2 * 0.7 + inst * 0.3 };
                *r = (Instant::now(), read, r.2);
            }
            r.2
        };
        let to_read = p.bytes_to_read.load(Relaxed);
        let paused = c.ctl.pause.load(Relaxed);
        let stage = {
            let s = c.stage.lock().unwrap().clone();
            if !s.is_empty() {
                s
            } else {
                match (c.job.kind(), p.phase.load(Relaxed)) {
                    ("restore", _) => "Restoring".into(),
                    (_, PHASE_LOOK) => "Looking".into(),
                    (_, PHASE_READ) if sent > 0 => "Sending".into(),
                    (_, PHASE_READ) => "Reading".into(),
                    (_, PHASE_COMMIT) => "Saving".into(),
                    _ => String::new(),
                }
            }
        };
        let phase = p.phase.load(Relaxed);
        let current = places::tilde(&p.current.lock().unwrap().clone());
        let status = JobStatus {
            id: c.id.clone(),
            kind: c.job.kind().into(),
            plan: c.job.plan().into(),
            plan_name,
            stage,
            files_seen: p.files_seen.load(Relaxed),
            files_to_read: p.files_to_read.load(Relaxed),
            files_read: p.files_read.load(Relaxed),
            bytes_to_read: to_read,
            bytes_read: read,
            sent_bytes: sent,
            dup_bytes: dup,
            current,
            started_at: c.started_at.clone(),
            paused,
            rate,
            eta_secs: if rate > 0.0 && (phase == PHASE_READ || c.job.kind() == "restore") && to_read > read { Some(((to_read - read) as f64 / rate) as u64) } else { None },
            queued: self.queue.lock().unwrap().len(),
        };
        Some(status)
    }

    /// The job thread, and a ticker sending progress twice a second while something runs.
    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            let next = {
                let mut q = me.queue.lock().unwrap();
                while q.is_empty() {
                    q = me.wake.wait(q).unwrap();
                }
                q.pop_front().unwrap()
            };
            me.run(next.0, next.1);
        });
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            if let Some(s) = me.status() {
                (me.emit)("job", serde_json::to_value(s).unwrap_or_default());
            }
        });
    }

    // ---- repositories ----

    fn plan_and_dest(&self, plan: &str) -> Result<(Plan, config::Destination), String> {
        let c = self.config.lock().unwrap();
        let p = c.plan(plan).cloned().ok_or("That plan no longer exists.")?;
        let d = c.destination(&p.destination).cloned().ok_or_else(|| format!("{} has no destination.", p.name))?;
        Ok((p, d))
    }

    /// The plan's repository, connecting its destination if needed. `create`: make it if this
    /// plan has never backed up; a plan that has must find its repository where it left it.
    pub fn repo(&self, plan: &str, create: bool) -> Result<Arc<Repo>, String> {
        let (p, d) = self.plan_and_dest(plan)?;
        let root = places::resolve(&d.place, &self.mounts, true, d.disconnect_after)?;
        if let Some((free, total)) = places::space(&root) {
            self.state.lock().unwrap().destinations.insert(d.id.clone(), config::DestState { free, total, checked: now() });
        }
        let path = root.join(&p.folder);
        if let Some((at, r)) = self.repos.lock().unwrap().get(plan) {
            if *at == path && r.backend.exists(CONFIG) {
                return Ok(r.clone());
            }
        }
        let folder = Folder::new(&path);
        let backend: Arc<dyn Backend> = if p.conditions.limit_mbps > 0 {
            Arc::new(Throttle { inner: folder, per_sec: p.conditions.limit_mbps as u64 * 1_000_000, sent: Mutex::new((Instant::now(), 0)) })
        } else {
            Arc::new(folder)
        };
        let st = self.state.lock().unwrap().plans.get(plan).cloned().unwrap_or_default();
        let password = if p.encrypted { keychain::get(&keychain::plan_account(plan)) } else { None };
        let repo = if backend.exists(CONFIG) {
            let cfg = Repo::read_config(backend.as_ref()).map_err(|e| e.0)?;
            let secret = match (&cfg.encryption, &password) {
                (Some(_), Some(pw)) => Secret::Password(pw),
                (Some(_), None) => return Err(format!("{}'s backup is encrypted, and its password isn't in the Keychain. Enter it in the plan's settings.", p.name)),
                (None, _) => Secret::None,
            };
            Repo::open(backend, secret).map_err(|e| e.0)?
        } else if st.created {
            return Err(format!(
                "{}'s backup isn't at {} any more. Keepr won't start a new one there by itself. If it was moved, put it back; to start again, choose Start a new backup in the plan.",
                p.name,
                places::tilde(&path.to_string_lossy())
            ));
        } else if !create {
            return Err(format!("{} hasn't backed up yet.", p.name));
        } else {
            if p.encrypted && password.is_none() {
                return Err(format!("{} is set to be encrypted but has no password. Set one in the plan.", p.name));
            }
            std::fs::create_dir_all(&path).map_err(|e| format!("Couldn't make {}: {e}", path.display()))?;
            let made = Repo::init(backend, password.as_deref()).map_err(|e| e.0)?;
            let mut s = self.state.lock().unwrap();
            let ps = s.plan(plan);
            ps.created = true;
            if let Some(rk) = &made.recovery_key {
                let _ = keychain::set(&keychain::recovery_account(plan), rk);
                ps.recovery_unsaved = true;
            }
            drop(s);
            self.save_state();
            made.repo
        };
        let repo = Arc::new(repo);
        self.repos.lock().unwrap().insert(plan.to_string(), (path, repo.clone()));
        Ok(repo)
    }

    pub fn snapshots(&self, plan: &str) -> Result<Vec<Snapshot>, String> {
        self.repo(plan, false)?.snapshots().map_err(|e| e.0)
    }

    // ---- running a job ----

    fn run(self: &Arc<Self>, id: String, job: Job) {
        let cur = Arc::new(Current {
            id: id.clone(),
            job: job.clone(),
            ctl: Arc::new(Control::default()),
            started_at: now(),
            stage: Mutex::new("Connecting".into()),
            repo: Mutex::new(None),
            base: Mutex::new((0, 0)),
            rate: Mutex::new((Instant::now(), 0, 0.0)),
        });
        *self.current.lock().unwrap() = Some(cur.clone());
        self.changed();
        let plan_name = self.config.lock().unwrap().plan(job.plan()).map(|p| p.name.clone()).unwrap_or_default();
        let mut run = Run { id, plan: job.plan().into(), kind: job.kind().into(), started: cur.started_at.clone(), finished: String::new(), result: "ok".into(), message: String::new(), files: 0, changed: 0, read_bytes: 0, added_bytes: 0, stored_bytes: 0, dup_bytes: 0 };
        let outcome = match &job {
            Job::Backup { plan, full } => self.backup(&cur, plan, *full, &mut run),
            Job::Restore { plan, snapshot, items, target, conflict } => self.restore(&cur, plan, snapshot, items, target, *conflict, &mut run),
            Job::Check { plan, all } => self.check(&cur, plan, *all, &mut run),
            Job::Prune { plan } => self.prune(&cur, plan, &mut run),
        };
        run.finished = now();
        match outcome {
            Ok(()) => {}
            Err(e) if e == "cancelled" => {
                run.result = "cancelled".into();
                run.message = "Stopped".into();
            }
            Err(e) => {
                let waiting = matches!(job, Job::Backup { .. }) && (e.contains("isn't connected") || e.contains("can't be reached"));
                let mut s = self.state.lock().unwrap();
                let was_waiting = s.plan(job.plan()).waiting.is_some();
                if waiting {
                    s.plan(job.plan()).waiting = Some(e.clone());
                }
                drop(s);
                run.result = if waiting { "waiting".into() } else { "failed".into() };
                run.message = e.clone();
                if !waiting && self.config.lock().unwrap().settings.notify_failures {
                    (self.notify)(&format!("{plan_name}: {} didn't finish", if job.kind() == "restore" { "restore" } else { "backup" }), &e);
                }
                if waiting && was_waiting {
                    // Still waiting: nothing new to tell anyone.
                    self.state.lock().unwrap().plan(job.plan()).last_attempt = Some(run.started.clone());
                    *self.current.lock().unwrap() = None;
                    self.save_state();
                    self.changed();
                    return;
                }
            }
        }
        {
            let mut s = self.state.lock().unwrap();
            s.plan(job.plan()).last_attempt = Some(run.started.clone());
            s.record(run);
        }
        self.save_state();
        *self.current.lock().unwrap() = None;
        self.changed();
    }

    fn set_stage(cur: &Current, s: &str) {
        *cur.stage.lock().unwrap() = s.to_string();
    }

    fn backup(self: &Arc<Self>, cur: &Arc<Current>, plan_id: &str, full: bool, run: &mut Run) -> Result<(), String> {
        let (plan, _) = self.plan_and_dest(plan_id)?;
        let repo = self.repo(plan_id, true)?;
        let mut sources = Vec::new();
        for s in &plan.sources {
            sources.push(places::resolve(s, &self.mounts, true, true)?);
        }
        let repo_path = self.repos.lock().unwrap().get(plan_id).map(|(p, _)| p.clone());
        *cur.base.lock().unwrap() = (repo.counters.stored_bytes.load(Relaxed), repo.counters.dup_bytes.load(Relaxed));
        *cur.repo.lock().unwrap() = Some(repo.clone());
        Self::set_stage(cur, "");

        let snaps = repo.snapshots().map_err(|e| e.0)?;
        let parent = snaps.last();
        let opts = Options {
            plan: plan_id.to_string(),
            sources,
            excludes: plan.excludes.clone(),
            gitignore: plan.gitignore,
            skip_dataless: plan.skip_cloud_only,
            max_file_size: (plan.max_file_size > 0).then_some(plan.max_file_size),
            full,
            skip_paths: repo_path.into_iter().collect(),
        };
        let snap = keepr_engine::backup::run(&repo, &opts, parent, &cur.ctl).map_err(|e| e.0)?;
        let st = &snap.stats;
        run.files = st.files;
        run.changed = st.new_files + st.changed_files;
        run.read_bytes = st.read_bytes;
        run.added_bytes = st.added_bytes;
        run.stored_bytes = st.stored_bytes;
        run.dup_bytes = st.dup_bytes;
        run.message = if run.changed == 0 { "Nothing changed".into() } else { format!("{} changed · {} sent", run.changed, human_bytes(st.stored_bytes)) };
        if st.error_count > 0 {
            run.result = "warning".into();
            run.message = format!("{} · {} couldn't be read", run.message, if st.error_count == 1 { "1 file".to_string() } else { format!("{} files", st.error_count) });
            if let Some(first) = st.errors.first() {
                run.message = format!("{} (first: {})", run.message, places::tilde(first));
            }
        }
        {
            let mut s = self.state.lock().unwrap();
            let ps = s.plan(plan_id);
            ps.last_success = Some(snap.time.clone());
            ps.waiting = None;
            ps.stale_warned = None;
            if full || parent.is_none() {
                ps.last_full = Some(now());
            }
        }
        self.refresh_stats(plan_id, &repo);
        if self.config.lock().unwrap().settings.notify_success {
            (self.notify)(&plan.name, &run.message);
        }

        // Tidy up once a day, and check when due, as part of the same visit to the destination.
        let due = |last: &Option<String>, hours: i64| last.as_deref().and_then(keepr_engine::retention::parse_time).is_none_or(|t| chrono::Local::now().signed_duration_since(t).num_hours() >= hours);
        let ps = self.state.lock().unwrap().plans.get(plan_id).cloned().unwrap_or_default();
        if due(&ps.last_prune, 20) && snaps.len() > 1 {
            self.enqueue(Job::Prune { plan: plan_id.into() });
        }
        let check_hours = match plan.check_every {
            Often::Weekly => Some(24 * 7),
            Often::Monthly => Some(24 * 30),
            Often::Never => None,
        };
        if let Some(h) = check_hours {
            if due(&ps.last_check, h) {
                self.enqueue(Job::Check { plan: plan_id.into(), all: plan.check_every == Often::Monthly });
            }
        }
        Ok(())
    }

    fn refresh_stats(&self, plan_id: &str, repo: &Repo) {
        let bytes: u64 = repo.index.read().unwrap().blobs.values().map(|l| l.len as u64).sum();
        let snaps = repo.snapshots().unwrap_or_default();
        let mut s = self.state.lock().unwrap();
        let ps = s.plan(plan_id);
        ps.repo_bytes = bytes;
        ps.snapshots = snaps.len() as u64;
        ps.versions_bytes = snaps.iter().map(|s| s.stats.bytes).sum();
        ps.oldest = snaps.first().map(|s| s.time.clone());
    }

    #[allow(clippy::too_many_arguments)]
    fn restore(&self, cur: &Arc<Current>, plan_id: &str, snapshot: &str, items: &[String], target: &Target, conflict: Conflict, run: &mut Run) -> Result<(), String> {
        let repo = self.repo(plan_id, false)?;
        let snap = repo.snapshots().map_err(|e| e.0)?.into_iter().find(|s| s.id.hex() == snapshot).ok_or("That snapshot is no longer in the backup.")?;
        *cur.repo.lock().unwrap() = Some(repo.clone());
        Self::set_stage(cur, "Restoring");
        let r = keepr_engine::restore::run(&repo, &snap, items, target, conflict, &cur.ctl).map_err(|e| e.0)?;
        run.files = r.files;
        run.read_bytes = r.bytes;
        let mut parts = vec![format!("{} restored", if r.files == 1 { "1 file".to_string() } else { format!("{} files", r.files) })];
        if r.renamed > 0 {
            parts.push(format!("{} kept beside the existing file", r.renamed));
        }
        if r.skipped > 0 {
            parts.push(format!("{} skipped", r.skipped));
        }
        if !r.errors.is_empty() {
            run.result = "warning".into();
            parts.push(format!("{} couldn't be written (first: {})", r.errors.len(), places::tilde(&r.errors[0])));
        }
        run.message = parts.join(" · ");
        Ok(())
    }

    fn check(&self, cur: &Arc<Current>, plan_id: &str, all: bool, run: &mut Run) -> Result<(), String> {
        let repo = self.repo(plan_id, false)?;
        Self::set_stage(cur, "Checking");
        let rep = keepr_engine::check::run(&repo, if all { 1.0 } else { 0.05 }, &cur.ctl).map_err(|e| e.0)?;
        self.state.lock().unwrap().plan(plan_id).last_check = Some(now());
        run.read_bytes = rep.bytes_read;
        if rep.problems.is_empty() {
            run.message = format!("All intact · {} snapshots{}", rep.snapshots, if all { ", all data read back".to_string() } else { format!(", {} of {} packs read back", rep.packs_read, rep.packs) });
        } else {
            run.result = "failed".into();
            run.message = format!("{} problem{}: {}", rep.problems.len(), if rep.problems.len() == 1 { "" } else { "s" }, rep.problems[0]);
            let name = self.config.lock().unwrap().plan(plan_id).map(|p| p.name.clone()).unwrap_or_default();
            (self.notify)(&format!("{name}: the backup check found problems"), &rep.problems[0]);
        }
        Ok(())
    }

    fn prune(&self, cur: &Arc<Current>, plan_id: &str, run: &mut Run) -> Result<(), String> {
        let (plan, _) = self.plan_and_dest(plan_id)?;
        let repo = self.repo(plan_id, false)?;
        Self::set_stage(cur, "Tidying up");
        let p = keepr_engine::prune::run(&repo, &plan.retention, &cur.ctl).map_err(|e| e.0)?;
        self.state.lock().unwrap().plan(plan_id).last_prune = Some(now());
        self.refresh_stats(plan_id, &repo);
        run.message = if p.forgotten == 0 && p.bytes_freed == 0 {
            "Nothing to remove".into()
        } else {
            format!("{} old snapshot{} removed · {} freed", p.forgotten, if p.forgotten == 1 { "" } else { "s" }, human_bytes(p.bytes_freed))
        };
        Ok(())
    }

    // ---- plans ----

    /// When the plan should next back up, from its schedule and last attempt.
    pub fn next_run(plan: &Plan, last: Option<&str>) -> Option<chrono::DateTime<chrono::Local>> {
        use chrono::{Datelike, Duration as D, Local, NaiveTime, TimeZone};
        let last = last.and_then(keepr_engine::retention::parse_time);
        let now = Local::now();
        let every = |d: D| Some(last.map_or(now, |l| l + d));
        match plan.schedule.every {
            Every::Manual => None,
            Every::Minutes15 => every(D::minutes(15)),
            Every::Hourly => every(D::hours(1)),
            Every::Daily | Every::Weekly => {
                let t = NaiveTime::parse_from_str(&plan.schedule.at, "%H:%M").unwrap_or(NaiveTime::from_hms_opt(2, 0, 0).unwrap());
                let slot = |date: chrono::NaiveDate| Local.from_local_datetime(&date.and_time(t)).earliest();
                let mut date = last.map_or(now, |l| l).date_naive();
                for _ in 0..16 {
                    if let Some(s) = slot(date) {
                        let weekday_ok = plan.schedule.every == Every::Daily || s.weekday().num_days_from_sunday() == plan.schedule.weekday % 7;
                        if weekday_ok && last.is_none_or(|l| s > l) {
                            return Some(s);
                        }
                    }
                    date = date.succ_opt()?;
                }
                None
            }
        }
    }

    pub fn full_due(plan: &Plan, last_full: Option<&str>) -> bool {
        let days = match plan.full_every {
            Often::Weekly => 7,
            Often::Monthly => 30,
            Often::Never => return false,
        };
        last_full.and_then(keepr_engine::retention::parse_time).is_none_or(|t| chrono::Local::now().signed_duration_since(t).num_days() >= days)
    }
}

pub fn describe_sources(plan: &Plan) -> String {
    plan.sources.iter().map(places::describe).collect::<Vec<_>>().join(", ")
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Destination, Schedule};

    fn core(dir: &std::path::Path) -> Arc<Core> {
        Core::new(dir.to_path_buf(), Box::new(|_, _| {}), Box::new(|_, _| {}), false)
    }

    fn plan(src: &std::path::Path) -> Plan {
        Plan {
            id: "p1".into(),
            name: "Docs".into(),
            enabled: true,
            sources: vec![Place::Folder { path: src.to_string_lossy().into() }],
            destination: "d1".into(),
            folder: "Docs p1".into(),
            schedule: Schedule { every: Every::Hourly, at: "02:00".into(), weekday: 0 },
            retention: Default::default(),
            excludes: vec![],
            gitignore: true,
            skip_cloud_only: true,
            max_file_size: 0,
            encrypted: false,
            full_every: Often::Weekly,
            check_every: Often::Weekly,
            conditions: Default::default(),
        }
    }

    fn wait(c: &Core) {
        for _ in 0..200 {
            std::thread::sleep(Duration::from_millis(25));
            if !c.running() && c.queue.lock().unwrap().is_empty() {
                return;
            }
        }
        panic!("job didn't finish");
    }

    #[test]
    fn backs_up_refuses_a_vanished_repo_and_restores() {
        let data = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.txt"), b"hello").unwrap();
        let c = core(data.path());
        {
            let mut cfg = c.config.lock().unwrap();
            cfg.destinations.push(Destination { id: "d1".into(), name: "Disk".into(), place: Place::Folder { path: dst.path().to_string_lossy().into() }, disconnect_after: true });
            cfg.plans.push(plan(src.path()));
        }
        c.start();
        c.enqueue(Job::Backup { plan: "p1".into(), full: false });
        wait(&c);
        let st = c.state.lock().unwrap().clone();
        assert_eq!(st.history[0].result, "ok", "{:?}", st.history);
        assert!(st.plans["p1"].created);
        assert_eq!(c.snapshots("p1").unwrap().len(), 1);

        // Restore into another folder.
        let out = tempfile::tempdir().unwrap();
        let snap = c.snapshots("p1").unwrap()[0].id.hex();
        c.enqueue(Job::Restore { plan: "p1".into(), snapshot: snap, items: vec![src.path().join("a.txt").to_string_lossy().into()], target: Target::Folder(out.path().into()), conflict: Conflict::KeepBoth });
        wait(&c);
        let name = src.path().file_name().unwrap();
        assert_eq!(std::fs::read(out.path().join(name).join("a.txt")).unwrap(), b"hello");

        // The repository disappears: the next backup fails rather than starting afresh.
        std::fs::remove_dir_all(dst.path().join("Docs p1")).unwrap();
        c.forget_repo("p1");
        c.enqueue(Job::Backup { plan: "p1".into(), full: false });
        wait(&c);
        let last = c.state.lock().unwrap().history.last().cloned().unwrap();
        assert_eq!(last.result, "failed");
        assert!(last.message.contains("won't start a new one"), "{}", last.message);
    }

    #[test]
    fn schedules() {
        let src = tempfile::tempdir().unwrap();
        let mut p = plan(src.path());
        let now = chrono::Local::now();
        assert!(Core::next_run(&p, None).unwrap() <= now + chrono::Duration::seconds(1));
        let last = (now - chrono::Duration::minutes(30)).to_rfc3339();
        let n = Core::next_run(&p, Some(&last)).unwrap();
        assert!(n > now && n < now + chrono::Duration::minutes(31));
        p.schedule.every = Every::Daily;
        p.schedule.at = "02:00".into();
        let n = Core::next_run(&p, Some(&now.to_rfc3339())).unwrap();
        assert!(n > now && n <= now + chrono::Duration::days(1));
        p.schedule.every = Every::Manual;
        assert!(Core::next_run(&p, None).is_none());
        assert!(Core::full_due(&p, None));
        assert!(!Core::full_due(&p, Some(&now.to_rfc3339())));
    }
}
