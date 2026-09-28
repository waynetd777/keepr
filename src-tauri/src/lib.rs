//! Keepr's app: the window's commands, the menu-bar item, and starting the job runner and the
//! scheduler. The backup engine itself is the keepr-engine crate (engine/).

mod aws_setup;
mod browse;
mod config;
mod core;
#[cfg(target_os = "macos")]
mod folder_panel;
mod keychain;
#[cfg(target_os = "macos")]
mod login_item;
#[cfg(target_os = "macos")]
mod login_launch;
mod places;
mod scheduler;
mod smb;
mod still;
mod fsevents;
mod system;

use crate::config::{Destination, Place, Plan, Run};
use crate::core::{Core, Job, JobStatus};
use serde::Serialize;
use std::sync::Arc;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

type Core_ = Arc<Core>;

const STATE_FLAGS: StateFlags = StateFlags::SIZE.union(StateFlags::POSITION).union(StateFlags::MAXIMIZED);

// ---- overview ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Day {
    added: u64,
    failed: bool,
    ran: bool,
    /// Backups that completed that day.
    count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanSummary {
    id: String,
    name: String,
    enabled: bool,
    sources: String,
    destination: String,
    destination_id: String,
    schedule: String,
    encrypted: bool,
    /// "running", "waiting", "failed", "stale", "never", "off" or "ok".
    status: String,
    message: String,
    last_success: Option<String>,
    last_attempt: Option<String>,
    next_run: Option<String>,
    repo_bytes: u64,
    versions_bytes: u64,
    snapshots: u64,
    oldest: Option<String>,
    days: Vec<Day>,
    recovery_unsaved: bool,
    last_changed: u64,
    icon: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DestSummary {
    id: String,
    name: String,
    /// "folder", "drive", "cloud", "smb" or "s3", for the icon; `kind_label` says it in words.
    kind: String,
    kind_label: String,
    place: String,
    /// "connected", "on demand" (a share Keepr connects when needed) or "missing".
    connection: String,
    free: Option<u64>,
    total: Option<u64>,
    keepr_bytes: u64,
    plans: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    plans: Vec<PlanSummary>,
    destinations: Vec<DestSummary>,
    stored_bytes: u64,
    versions_bytes: u64,
    job: Option<JobStatus>,
    /// Jobs waiting their turn.
    queued: Vec<Queued>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Queued {
    id: String,
    plan: String,
    kind: String,
}

fn schedule_label(p: &Plan) -> String {
    use crate::config::Every::*;
    match p.schedule.every {
        Minutes15 => "Every 15 min".into(),
        Hourly => "Hourly".into(),
        Daily => format!("Daily at {}", p.schedule.at),
        Weekly => format!("{}s at {}", ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][p.schedule.weekday as usize % 7], p.schedule.at),
        Manual => "Only when asked".into(),
    }
}

fn icon_for(p: &Plan) -> String {
    let s = core::describe_sources(p).to_lowercase();
    if s.contains("picture") || s.contains("photo") {
        "photos".into()
    } else if p.sources.len() == 1 && s.contains("note") {
        "notes".into()
    } else {
        "folder".into()
    }
}

fn overview_of(core: &Core) -> Overview {
    let cfg = core.config.lock().unwrap().clone();
    let st = core.state.lock().unwrap().clone();
    let job = core.status();
    let now = chrono::Local::now();
    let mut plans = Vec::new();
    for p in &cfg.plans {
        let ps = st.plans.get(&p.id).cloned().unwrap_or_default();
        let runs: Vec<&Run> = st.history.iter().filter(|r| r.plan == p.id).collect();
        let last_backup = runs.iter().rev().find(|r| r.kind == "backup" || r.kind == "full");
        let running = job.as_ref().is_some_and(|j| j.plan == p.id);
        let stale = ps.last_success.as_deref().and_then(keepr_engine::retention::parse_time).is_some_and(|t| now.signed_duration_since(t).num_days() >= cfg.settings.stale_days as i64);
        let (status, message) = if !p.enabled {
            ("off", "Turned off".to_string())
        } else if running {
            ("running", job.as_ref().map(|j| j.stage.clone()).unwrap_or_default())
        } else if let Some(w) = &ps.waiting {
            ("waiting", w.clone())
        } else if last_backup.is_some_and(|r| r.result == "failed") {
            ("failed", last_backup.unwrap().message.clone())
        } else if !ps.created || ps.last_success.is_none() {
            ("never", "Hasn't backed up yet".to_string())
        } else if stale {
            ("stale", "No backup for a while".to_string())
        } else {
            ("ok", last_backup.map(|r| r.message.clone()).unwrap_or_default())
        };
        let mut days: Vec<Day> = (0..30).map(|_| Day { added: 0, failed: false, ran: false, count: 0 }).collect();
        for r in &runs {
            if r.kind != "backup" && r.kind != "full" {
                continue;
            }
            let Some(t) = keepr_engine::retention::parse_time(&r.started) else { continue };
            let ago = (now.date_naive() - t.date_naive()).num_days();
            if !(0..30).contains(&ago) {
                continue;
            }
            let d = &mut days[29 - ago as usize];
            d.ran = true;
            d.added += r.stored_bytes;
            d.failed |= r.result == "failed";
            if r.result == "ok" || r.result == "warning" {
                d.count += 1;
            }
        }
        let dest = cfg.destination(&p.destination);
        plans.push(PlanSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            enabled: p.enabled,
            sources: core::source_names(p).join(", "),
            destination: dest.map(|d| d.name.clone()).unwrap_or_default(),
            destination_id: p.destination.clone(),
            schedule: schedule_label(p),
            encrypted: p.encrypted,
            status: status.into(),
            message,
            last_success: ps.last_success.clone(),
            last_attempt: ps.last_attempt.clone(),
            next_run: if p.enabled { Core::next_run(p, ps.last_attempt.as_deref()).map(|t| t.to_rfc3339()) } else { None },
            repo_bytes: ps.repo_bytes,
            versions_bytes: ps.versions_bytes,
            snapshots: ps.snapshots,
            oldest: ps.oldest.clone(),
            days,
            recovery_unsaved: ps.recovery_unsaved,
            last_changed: last_backup.map(|r| r.changed).unwrap_or(0),
            icon: icon_for(p),
        });
    }
    let mut destinations = Vec::new();
    for d in &cfg.destinations {
        let users: Vec<&Plan> = cfg.plans.iter().filter(|p| p.destination == d.id).collect();
        let (kind, kind_label) = places::kind_of(&d.place);
        let (connection, path) = match &d.place {
            Place::Folder { .. } => match places::resolve(&d.place, &core.mounts, false, true) {
                Ok(p) => ("connected", Some(p)),
                Err(_) => ("missing", None),
            },
            Place::Smb(s) => match smb::find_mount(&s.server, &s.share) {
                Some(m) => ("connected", Some(m)),
                None => ("on demand", None),
            },
            Place::S3(_) => ("on demand", None),
        };
        let space = path.as_deref().and_then(places::space).or_else(|| st.destinations.get(&d.id).map(|s| (s.free, s.total)));
        destinations.push(DestSummary {
            id: d.id.clone(),
            name: d.name.clone(),
            kind: kind.into(),
            kind_label,
            place: places::describe(&d.place),
            connection: connection.into(),
            free: space.map(|s| s.0),
            total: space.map(|s| s.1),
            keepr_bytes: users.iter().map(|p| st.plans.get(&p.id).map_or(0, |s| s.repo_bytes)).sum(),
            plans: users.iter().map(|p| p.name.clone()).collect(),
        });
    }
    Overview {
        stored_bytes: plans.iter().map(|p| p.repo_bytes).sum(),
        versions_bytes: plans.iter().map(|p| p.versions_bytes).sum(),
        plans,
        destinations,
        job,
        queued: core.queued().into_iter().map(|(id, plan, kind)| Queued { id, plan, kind }).collect(),
    }
}

#[tauri::command]
fn overview(core: State<Core_>) -> Overview {
    overview_of(&core)
}

// ---- configuration ----

#[tauri::command]
fn get_config(core: State<Core_>) -> config::Config {
    core.config.lock().unwrap().clone()
}

/// A run's log (Activity, when a run is expanded). Empty for runs from before logs were kept.
#[tauri::command]
fn run_log(core: State<Core_>, id: String) -> Vec<String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return vec![];
    }
    std::fs::read_to_string(core.dir.join("logs").join(format!("{id}.log"))).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default()
}

/// Runs newest first: `limit` of them after skipping `offset`, filtered to "all", "problems"
/// (anything not ok) or "restores".
#[tauri::command]
fn history(core: State<Core_>, offset: usize, limit: usize, filter: Option<String>) -> Vec<Run> {
    let st = core.state.lock().unwrap();
    let keep = |r: &&Run| match filter.as_deref() {
        Some("problems") => r.result != "ok",
        Some("restores") => r.kind == "restore",
        _ => true,
    };
    st.history.iter().rev().filter(keep).skip(offset).take(limit).cloned().collect()
}

#[tauri::command]
fn default_excludes() -> Vec<String> {
    config::default_excludes()
}

#[tauri::command]
fn save_plan(core: State<Core_>, mut plan: Plan, password: Option<String>) -> Result<Plan, String> {
    if plan.name.trim().is_empty() {
        return Err("Give the plan a name.".into());
    }
    if plan.sources.is_empty() {
        return Err("Add at least one folder to back up.".into());
    }
    if core.config.lock().unwrap().destination(&plan.destination).is_none() {
        return Err("Choose where to keep the backup.".into());
    }
    if plan.id.is_empty() {
        plan.id = config::new_id();
        plan.folder = config::folder_name(&plan.name, &plan.id);
    }
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        keychain::set(&keychain::plan_account(&plan.id), &pw)?;
    }
    if plan.encrypted && keychain::get(&keychain::plan_account(&plan.id)).is_none() {
        return Err("An encrypted plan needs a password.".into());
    }
    {
        let mut c = core.config.lock().unwrap();
        // A backup's encryption is fixed when it is made.
        let created = core.state.lock().unwrap().plans.get(&plan.id).is_some_and(|s| s.created);
        if let Some(old) = c.plans.iter_mut().find(|p| p.id == plan.id) {
            if created && (old.encrypted != plan.encrypted || old.destination != plan.destination) {
                return Err(if old.encrypted != plan.encrypted {
                    "Encryption can't be turned on or off for a backup that already exists. Make a new plan instead.".into()
                } else {
                    "A plan's backup can't move to another destination. Make a new plan instead.".into()
                });
            }
            *old = plan.clone();
        } else {
            c.plans.push(plan.clone());
        }
    }
    core.save_config()?;
    core.forget_repo(&plan.id);
    core.changed();
    Ok(plan)
}

#[tauri::command]
fn delete_plan(core: State<Core_>, id: String) -> Result<(), String> {
    if core.busy_with(&id) {
        return Err("Wait for this plan's backup to finish first.".into());
    }
    core.config.lock().unwrap().plans.retain(|p| p.id != id);
    core.state.lock().unwrap().plans.remove(&id);
    keychain::delete(&keychain::plan_account(&id));
    keychain::delete(&keychain::recovery_account(&id));
    core.forget_repo(&id);
    core.save_config()?;
    core.save_state();
    core.changed();
    Ok(())
}

#[tauri::command]
fn set_plan_enabled(core: State<Core_>, id: String, enabled: bool) -> Result<(), String> {
    if let Some(p) = core.config.lock().unwrap().plans.iter_mut().find(|p| p.id == id) {
        p.enabled = enabled;
    }
    core.save_config()?;
    core.changed();
    Ok(())
}

/// Lets a plan whose repository has gone start a new one.
#[tauri::command]
fn start_new_backup(core: State<Core_>, id: String) -> Result<(), String> {
    let mut s = core.state.lock().unwrap();
    let ps = s.plan(&id);
    ps.created = false;
    ps.waiting = None;
    drop(s);
    core.forget_repo(&id);
    core.save_state();
    core.changed();
    Ok(())
}

#[tauri::command]
fn save_destination(core: State<Core_>, mut dest: Destination, password: Option<String>) -> Result<Destination, String> {
    dest.name = dest.name.trim().to_string();
    if dest.name.is_empty() {
        dest.name = places::default_name(&dest.place);
    }
    {
        let c = core.config.lock().unwrap();
        if let Some(other) = c.destinations.iter().find(|d| d.id != dest.id && d.name.eq_ignore_ascii_case(&dest.name)) {
            return Err(format!("Another destination is already called {}. Give this one a different name.", other.name));
        }
    }
    match (&dest.place, password.filter(|p| !p.is_empty())) {
        (Place::Smb(s), Some(pw)) => keychain::set(&keychain::smb_account(&s.user, &s.server), &pw)?,
        (Place::S3(s), Some(pw)) => keychain::set(&keychain::s3_account(&s.access_key), &pw)?,
        (Place::S3(s), None) if keychain::get(&keychain::s3_account(&s.access_key)).is_none() => return Err("Enter the bucket's secret key.".into()),
        _ => {}
    }
    if dest.id.is_empty() {
        dest.id = config::new_id();
    }
    {
        let mut c = core.config.lock().unwrap();
        match c.destinations.iter_mut().find(|d| d.id == dest.id) {
            Some(d) => *d = dest.clone(),
            None => c.destinations.push(dest.clone()),
        }
        let users: Vec<String> = c.plans.iter().filter(|p| p.destination == dest.id).map(|p| p.id.clone()).collect();
        drop(c);
        for u in users {
            core.forget_repo(&u);
        }
    }
    core.save_config()?;
    core.changed();
    Ok(dest)
}

/// A name for a new destination that says where it is and isn't taken.
#[tauri::command]
fn suggest_name(core: State<Core_>, place: Place, except: Option<String>) -> String {
    let taken: Vec<String> = core.config.lock().unwrap().destinations.iter().filter(|d| Some(&d.id) != except.as_ref()).map(|d| d.name.clone()).collect();
    places::unique_name(&places::default_name(&place), &taken)
}

#[tauri::command]
fn delete_destination(core: State<Core_>, id: String) -> Result<(), String> {
    let mut c = core.config.lock().unwrap();
    if let Some(p) = c.plans.iter().find(|p| p.destination == id) {
        return Err(format!("{} keeps its backup here. Delete that plan first.", p.name));
    }
    c.destinations.retain(|d| d.id != id);
    drop(c);
    core.save_config()?;
    core.changed();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tested {
    ok: bool,
    message: String,
    free: Option<u64>,
    total: Option<u64>,
    mbps: Option<f64>,
}

/// Connects to a destination, writes and removes a test file, and measures the speed.
#[tauri::command]
async fn test_place(core: State<'_, Core_>, place: Place, password: Option<String>) -> Result<Tested, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let fail = |m: String| Tested { ok: false, message: m, free: None, total: None, mbps: None };
        if let Place::S3(s) = &place {
            return match test_bucket(s, password) {
                Ok(mbps) => Tested { ok: true, message: "Connected, and Keepr can write to this bucket.".into(), free: None, total: None, mbps: Some(mbps) },
                Err(e) => fail(e),
            };
        }
        let base = match &place {
            Place::Smb(s) => {
                let pw = password.filter(|p| !p.is_empty()).or_else(|| places::smb_password(s)).unwrap_or_default();
                match smb::find_mount(&s.server, &s.share).map(Ok).unwrap_or_else(|| smb::mount(&s.server, &s.share, &s.user, &pw)) {
                    Ok(m) => {
                        let folder = s.folder.trim_matches('/');
                        if folder.is_empty() { m } else { m.join(folder) }
                    }
                    Err(e) => return fail(e),
                }
            }
            p => match places::resolve(p, &core.mounts, false, true) {
                Ok(p) => p,
                Err(e) => return fail(e),
            },
        };
        if let Err(e) = std::fs::create_dir_all(&base) {
            return fail(format!("Keepr can't make its folder there: {e}"));
        }
        let probe = base.join(format!(".keepr-test-{}", config::new_id()));
        let data = vec![0x5au8; 8 << 20];
        let t = std::time::Instant::now();
        let res = std::fs::write(&probe, &data).and_then(|_| keepr_engine::backend::sync(&std::fs::OpenOptions::new().write(true).open(&probe)?));
        let secs = t.elapsed().as_secs_f64();
        let _ = std::fs::remove_file(&probe);
        if let Err(e) = res {
            return fail(format!("Keepr can't write there: {e}"));
        }
        let space = places::space(&base);
        Tested { ok: true, message: "Connected, and Keepr can write here.".into(), free: space.map(|s| s.0), total: space.map(|s| s.1), mbps: Some(8.0 * 1.048_576 / secs.max(0.001)) }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Checks a bucket and writes, reads back and removes a test object. Returns the upload speed.
fn test_bucket(s: &config::S3, secret: Option<String>) -> Result<f64, String> {
    use keepr_engine::backend::Backend;
    let b = places::s3_backend(s, secret, "")?;
    b.check_bucket().map_err(|e| e.to_string())?;
    let name = format!(".keepr-test-{}", config::new_id());
    let data = vec![0x5au8; 4 << 20];
    let t = std::time::Instant::now();
    b.write(&name, &data).map_err(|e| format!("Keepr can't write to the bucket: {e}"))?;
    let secs = t.elapsed().as_secs_f64();
    let back = b.read_at(&name, 0, 16).map_err(|e| format!("Keepr can't read from the bucket: {e}"));
    let _ = b.remove(&name);
    back?;
    Ok(4.0 * 1.048_576 / secs.max(0.001))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AwsSetupInfo {
    cli: bool,
    bucket: String,
}

/// Whether Keepr can sign in to AWS through the browser (the AWS CLI is installed), and a free-looking bucket name.
#[tauri::command]
async fn aws_setup_info() -> AwsSetupInfo {
    let cli = tauri::async_runtime::spawn_blocking(|| aws_setup::cli().is_some()).await.unwrap_or(false);
    AwsSetupInfo { cli, bucket: aws_setup::suggest_bucket() }
}

/// Signs in through the browser and makes the bucket and its user.
#[tauri::command]
async fn aws_setup_run(region: String, bucket: String) -> Result<aws_setup::Made, String> {
    tauri::async_runtime::spawn_blocking(move || aws_setup::with_cli(region.trim(), bucket.trim())).await.map_err(|e| e.to_string())?
}

/// The same setup as a script to paste into AWS CloudShell.
#[tauri::command]
fn aws_setup_script(region: String, bucket: String) -> Result<String, String> {
    aws_setup::script(region.trim(), bucket.trim())
}

/// The line CloudShell printed at the end.
#[tauri::command]
async fn aws_setup_paste(text: String) -> Result<aws_setup::Made, String> {
    tauri::async_runtime::spawn_blocking(move || aws_setup::parse(&text).and_then(aws_setup::finish)).await.map_err(|e| e.to_string())?
}

/// SMB servers to offer: ones Keepr already uses, ones mounted now, and ones on Bonjour.
/// The cloud services' sync folders on this Mac, for Add a destination.
#[tauri::command]
async fn cloud_folders() -> Vec<places::CloudFolder> {
    tauri::async_runtime::spawn_blocking(places::cloud_folders).await.unwrap_or_default()
}

#[tauri::command]
async fn discover_servers(core: State<'_, Core_>) -> Result<Vec<String>, String> {
    let mut known: Vec<String> = {
        let c = core.config.lock().unwrap();
        c.destinations.iter().map(|d| &d.place).chain(c.plans.iter().flat_map(|p| p.sources.iter())).filter_map(|p| if let Place::Smb(s) = p { Some(s.server.clone()) } else { None }).collect()
    };
    known.extend(smb::mounted_servers());
    let found = tauri::async_runtime::spawn_blocking(smb::discover).await.unwrap_or_default();
    known.extend(found.into_iter().map(|n| format!("{n}.local")));
    let mut seen = std::collections::HashSet::new();
    known.retain(|s| seen.insert(s.to_lowercase()));
    Ok(known)
}

#[tauri::command]
async fn list_shares(server: String, user: String, password: Option<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let pw = password
            .filter(|p| !p.is_empty())
            .or_else(|| keychain::get(&keychain::smb_account(&user, &server)))
            .or_else(|| keychain::finder_smb_password(&server, &user))
            .unwrap_or_default();
        smb::shares(&server, &user, &pw)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedLogin {
    user: String,
    /// "keepr" or "finder".
    source: String,
}

/// A login already saved for this server, so the dialogs can fill it in.
#[tauri::command]
async fn saved_smb_login(server: String) -> Option<SavedLogin> {
    tauri::async_runtime::spawn_blocking(move || keychain::saved_smb_user(&server).map(|(user, source)| SavedLogin { user, source: source.into() })).await.ok().flatten()
}

#[tauri::command]
fn save_smb_password(server: String, user: String, password: String) -> Result<(), String> {
    keychain::set(&keychain::smb_account(&user, &server), &password)
}

#[tauri::command]
fn has_password(account_kind: String, id: String, user: Option<String>) -> bool {
    let acct = match account_kind.as_str() {
        "plan" => keychain::plan_account(&id),
        _ => keychain::smb_account(user.as_deref().unwrap_or(""), &id),
    };
    keychain::get(&acct).is_some()
}

#[tauri::command]
fn recovery_key(id: String) -> Option<String> {
    keychain::get(&keychain::recovery_account(&id))
}

#[tauri::command]
fn recovery_saved(core: State<Core_>, id: String) {
    core.state.lock().unwrap().plan(&id).recovery_unsaved = false;
    core.save_state();
    core.changed();
}

#[tauri::command]
async fn change_password(core: State<'_, Core_>, id: String, current: String, new: String) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if new.len() < 8 {
            return Err("Use at least 8 characters.".to_string());
        }
        let repo = core.repo(&id, false)?;
        let backend = repo.backend.clone();
        drop(repo);
        core.forget_repo(&id);
        let secret = if current.contains('-') && current.len() > 40 { keepr_engine::repo::Secret::RecoveryKey(&current) } else { keepr_engine::repo::Secret::Password(&current) };
        let mut r = keepr_engine::repo::Repo::open(backend, secret).map_err(|e| e.0)?;
        r.change_password(if current.contains('-') && current.len() > 40 { keepr_engine::repo::Secret::RecoveryKey(&current) } else { keepr_engine::repo::Secret::Password(&current) }, &new).map_err(|e| e.0)?;
        keychain::set(&keychain::plan_account(&id), &new)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- jobs ----

#[tauri::command]
fn back_up(core: State<Core_>, plan: String, full: bool) -> String {
    core.state.lock().unwrap().plan(&plan).waiting = None;
    core.enqueue(Job::Backup { plan, full })
}

#[tauri::command]
fn back_up_all(core: State<Core_>) {
    let ids: Vec<String> = core.config.lock().unwrap().plans.iter().filter(|p| p.enabled).map(|p| p.id.clone()).collect();
    for id in ids {
        core.state.lock().unwrap().plan(&id).waiting = None;
        core.enqueue(Job::Backup { plan: id, full: false });
    }
}

#[tauri::command]
fn restore(core: State<Core_>, plan: String, snapshot: String, items: Vec<String>, target: keepr_engine::restore::Target, conflict: keepr_engine::restore::Conflict) -> String {
    core.enqueue(Job::Restore { plan, snapshot, items, target, conflict })
}

/// Takes a source's data out of every snapshot of the plan (after the user has confirmed).
#[tauri::command]
fn remove_source_data(core: State<Core_>, plan: String, source: Place) -> String {
    core.enqueue(Job::RemoveSource { plan, source })
}

/// Takes a file or folder out of every snapshot of the plan (after the user has confirmed).
#[tauri::command]
fn remove_path_data(core: State<Core_>, plan: String, path: String) -> String {
    core.enqueue(Job::RemovePath { plan, path })
}

#[tauri::command]
fn check_now(core: State<Core_>, plan: String, all: bool) -> String {
    core.enqueue(Job::Check { plan, all })
}

#[tauri::command]
fn job_status(core: State<Core_>) -> Option<JobStatus> {
    core.status()
}

#[tauri::command]
fn job_cancel(core: State<Core_>, id: String) {
    core.cancel(&id)
}

#[tauri::command]
fn job_pause(core: State<Core_>, paused: bool) {
    core.pause(paused)
}

/// Pauses scheduled backups for an hour: plans count as just attempted.
#[tauri::command]
fn pause_hour(core: State<Core_>) {
    let later = (chrono::Local::now() + chrono::Duration::minutes(60)).to_rfc3339();
    let ids: Vec<String> = core.config.lock().unwrap().plans.iter().map(|p| p.id.clone()).collect();
    let mut s = core.state.lock().unwrap();
    for id in ids {
        s.plan(&id).last_attempt = Some(later.clone());
    }
    drop(s);
    core.save_state();
    core.cancel("");
    core.changed();
}

// ---- restore browsing ----

#[tauri::command]
async fn snapshots(core: State<'_, Core_>, plan: String) -> Result<Vec<browse::SnapInfo>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || Ok(core.snapshots(&plan)?.iter().map(browse::info).collect())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_dir(core: State<'_, Core_>, plan: String, snapshot: String, path: String, show_deleted: bool) -> Result<Vec<browse::Entry>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || browse::list(&core, &plan, &snapshot, &path, show_deleted)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn file_versions(core: State<'_, Core_>, plan: String, path: String) -> Result<Vec<browse::VersionInfo>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || browse::versions(&core, &plan, &path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn search_snapshot(core: State<'_, Core_>, plan: String, snapshot: String, query: String) -> Result<Vec<browse::Entry>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || browse::search(&core, &plan, &snapshot, &query)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn compare(core: State<'_, Core_>, plan: String, snapshot: String, path: String) -> Result<browse::Comparison, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || browse::compare(&core, &plan, &snapshot, &path)).await.map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Everywhere {
    found: Vec<browse::Found>,
    /// Plans that couldn't be searched (their destination isn't reachable).
    missed: Vec<String>,
}

#[tauri::command]
async fn search_everywhere(core: State<'_, Core_>, query: String) -> Result<Everywhere, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (found, missed) = browse::search_everywhere(&core, &query);
        Everywhere { found, missed }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn quick_look(core: State<'_, Core_>, plan: String, snapshot: String, path: String) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || browse::quick_look(&core, &plan, &snapshot, &path)).await.map_err(|e| e.to_string())?
}

// ---- the app ----

/// The folder panel (see folder_panel.rs), run on the main thread.
#[tauri::command]
async fn choose_folders(app: AppHandle, title: String, multiple: bool, start: Option<String>) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let mtm = objc2::MainThreadMarker::new().expect("on the main thread");
            let _ = tx.send(folder_panel::choose(mtm, &title, multiple, start.as_deref()));
        });
        tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or_default()).await.unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, title, multiple, start);
        vec![]
    }
}

#[tauri::command]
fn app_version() -> (String, String) {
    (env!("CARGO_PKG_VERSION").to_string(), option_env!("KEEPR_BUILD").unwrap_or("dev").to_string())
}

#[tauri::command]
fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

#[tauri::command]
fn login_item() -> (bool, bool) {
    #[cfg(target_os = "macos")]
    {
        (login_item::available(), login_item::status().is_on())
    }
    #[cfg(not(target_os = "macos"))]
    (false, false)
}

#[tauri::command]
fn set_login_item(on: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        use login_item::Status;
        let s = login_item::set(on);
        if s == Status::RequiresApproval {
            login_item::open_settings();
        }
        s.is_on()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on;
        false
    }
}

#[tauri::command]
fn save_settings(core: State<Core_>, settings: config::Settings) -> Result<(), String> {
    core.config.lock().unwrap().settings = settings;
    core.save_config()?;
    core.changed();
    Ok(())
}

/// Screenshot mode: the scene to set up, from KEEPR_SCENE.
#[tauri::command]
fn scene() -> Option<String> {
    std::env::var("KEEPR_SCENE").ok().filter(|s| !s.is_empty())
}

#[tauri::command]
fn show_main_window(app: AppHandle, screen: Option<String>) {
    show_main(&app);
    if let Some(s) = screen {
        let _ = app.emit("navigate", s);
    }
    if let Some(t) = app.get_webview_window("tray") {
        let _ = t.hide();
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    let _ = app.save_window_state(STATE_FLAGS);
    app.exit(0)
}

#[cfg(target_os = "macos")]
fn set_in_dock(app: &AppHandle, shown: bool) {
    let _ = app.set_activation_policy(if shown { tauri::ActivationPolicy::Regular } else { tauri::ActivationPolicy::Accessory });
}

#[cfg(not(target_os = "macos"))]
fn set_in_dock(_app: &AppHandle, _shown: bool) {}

/// Brings the app to the front: after coming back from Accessory, `set_focus()` alone can leave
/// the window behind whatever the user was working in.
#[cfg(target_os = "macos")]
fn activate() {
    use objc2::runtime::{AnyClass, AnyObject};
    let Some(cls) = AnyClass::get(c"NSApplication") else { return };
    unsafe {
        let nsapp: *mut AnyObject = objc2::msg_send![cls, sharedApplication];
        if !nsapp.is_null() {
            let _: () = objc2::msg_send![nsapp, activateIgnoringOtherApps: true];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn activate() {}

fn show_main(app: &AppHandle) {
    // Back in the Dock before the window is shown: done afterwards, the window can come up
    // behind whatever had focus.
    set_in_dock(app, true);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        activate();
        let _ = w.set_focus();
    }
}

/// The menu-bar icon: plain, with a dot while backing up, with a mark when something needs attention.
fn tray_icon_for(core: &Core) -> &'static [u8] {
    let o = overview_of(core);
    if o.job.is_some() {
        include_bytes!("../icons/tray-busy@2x.png")
    } else if o.plans.iter().any(|p| matches!(p.status.as_str(), "failed" | "waiting" | "stale")) {
        include_bytes!("../icons/tray-alert@2x.png")
    } else {
        include_bytes!("../icons/tray@2x.png")
    }
}

fn toggle_tray_window(app: &AppHandle, rect: tauri::Rect) {
    let Some(w) = app.get_webview_window("tray") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    // The icon's place is in physical pixels across all screens. Work out which screen it's on
    // and use that screen's scale: the window's own scale is the screen it was last on, which
    // with two displays puts it on the wrong one.
    let guess = w.scale_factor().unwrap_or(2.0);
    let (pos, size) = (rect.position.to_physical::<f64>(guess), rect.size.to_physical::<f64>(guess));
    let monitors = app.available_monitors().unwrap_or_default();
    let screen = monitors.iter().find(|m| {
        let (p, s) = (m.position(), m.size());
        pos.x >= p.x as f64 && pos.x < (p.x + s.width as i32) as f64 && pos.y >= p.y as f64 && pos.y < (p.y + s.height as i32) as f64
    });
    let scale = screen.map_or(guess, |m| m.scale_factor());
    let (pos, size) = if (scale - guess).abs() > f64::EPSILON { (rect.position.to_physical::<f64>(scale), rect.size.to_physical::<f64>(scale)) } else { (pos, size) };
    let width = 360.0 * scale;
    let mut x = pos.x + size.width / 2.0 - width / 2.0;
    if let Some(m) = screen {
        let right = (m.position().x + m.size().width as i32) as f64;
        x = x.min(right - width - 8.0 * scale).max(m.position().x as f64 + 8.0 * scale);
    }
    let y = pos.y + size.height + 6.0 * scale;
    let place = tauri::PhysicalPosition::new(x, y);
    let _ = w.set_position(place);
    let _ = w.show();
    // Once on that screen, place it again: macOS converts the first move with the old screen's scale.
    let _ = w.set_position(place);
    let _ = w.set_focus();
    let _ = w.emit("tray-opened", ());
}

/// `Keepr --back-up <plan id>…`: runs those backups without a window and exits, for scripts
/// (tools/screenshots.py makes its demo backups this way). Exit status 1 if any didn't complete.
pub fn cli(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        // Copies one plan's backup password to another, so a new plan can share it.
        Some("--copy-plan-password") if args.len() == 3 => {
            let Some(pw) = keychain::get(&keychain::plan_account(&args[1])) else {
                eprintln!("no password for plan {}", args[1]);
                return Some(1);
            };
            return Some(match keychain::set(&keychain::plan_account(&args[2]), &pw) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            });
        }
        // Takes a path out of every snapshot of a plan, as Restore's Remove from backup does.
        Some("--remove-path") if args.len() == 3 => {
            let core = Core::new(config::data_dir(), Box::new(|_, _| {}), Box::new(|t, b| eprintln!("{t}: {b}")), false);
            core.start();
            core.enqueue(Job::RemovePath { plan: args[1].clone(), path: args[2].clone() });
            std::thread::sleep(std::time::Duration::from_millis(200));
            while core.running() || core.busy_with_any() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let r = core.state.lock().unwrap().history.last().cloned();
            if let Some(r) = &r {
                println!("{} {}: {}", r.kind, r.result, r.message);
            }
            return Some(if r.is_some_and(|r| r.result == "ok") { 0 } else { 1 });
        }
        Some("--back-up") => {}
        _ => return None,
    }
    let core = Core::new(config::data_dir(), Box::new(|_, _| {}), Box::new(|t, b| eprintln!("{t}: {b}")), false);
    core.start();
    let before = core.state.lock().unwrap().history.len();
    for id in &args[1..] {
        core.enqueue(Job::Backup { plan: id.clone(), full: false });
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    while core.running() || core.busy_with_any() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let st = core.state.lock().unwrap();
    let mut ok = true;
    for r in &st.history[before..] {
        println!("{} {}: {}", r.kind, r.result, r.message);
        ok &= r.result == "ok" || r.result == "warning";
    }
    Some(if ok { 0 } else { 1 })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let frozen = scene().is_some();
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().with_state_flags(STATE_FLAGS).with_denylist(&["tray"]).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            overview,
            aws_setup_info,
            aws_setup_run,
            aws_setup_script,
            aws_setup_paste,
            get_config,
            history,
            run_log,
            default_excludes,
            save_plan,
            delete_plan,
            set_plan_enabled,
            start_new_backup,
            save_destination,
            delete_destination,
            suggest_name,
            test_place,
            discover_servers,
            cloud_folders,
            list_shares,
            save_smb_password,
            saved_smb_login,
            has_password,
            recovery_key,
            recovery_saved,
            change_password,
            back_up,
            back_up_all,
            restore,
            check_now,
            remove_source_data,
            remove_path_data,
            job_status,
            job_cancel,
            job_pause,
            pause_hour,
            snapshots,
            list_dir,
            file_versions,
            search_snapshot,
            search_everywhere,
            quick_look,
            compare,
            app_version,
            choose_folders,
            home_dir,
            login_item,
            set_login_item,
            save_settings,
            scene,
            show_main_window,
            quit
        ])
        .setup(move |app| {
            // Did Login Items start this, rather than someone opening the app? Asked first: the
            // answer is in the launch AppleEvent AppKit is dispatching now. A login launch stays in
            // the menu bar.
            #[cfg(target_os = "macos")]
            let quiet = login_launch::probe() && !frozen;
            #[cfg(not(target_os = "macos"))]
            let quiet = false;
            if quiet {
                set_in_dock(app.handle(), false);
            }

            let handle = app.handle().clone();
            let emitter = handle.clone();
            let notifier = handle.clone();
            let core = Core::new(
                config::data_dir(),
                Box::new(move |name, payload| {
                    let _ = emitter.emit(name, payload);
                    if name == "changed" {
                        let h = emitter.clone();
                        let _ = emitter.run_on_main_thread(move || {
                            if let (Some(c), Some(t)) = (h.try_state::<Core_>(), h.tray_by_id("main")) {
                                if let Ok(img) = tauri::image::Image::from_bytes(tray_icon_for(&c)) {
                                    let _ = t.set_icon(Some(img));
                                    let _ = t.set_icon_as_template(true);
                                }
                            }
                        });
                    }
                }),
                Box::new(move |title, body| {
                    if let Err(e) = notifier.notification().builder().title(title).body(body).show() {
                        eprintln!("notification: {e}");
                    }
                }),
                frozen,
            );
            app.manage(core.clone());
            if !frozen {
                std::thread::spawn(still::clean_up_left_behind);
            }
            core.start();
            scheduler::start(core.clone());
            system::watch_network();

            let icon = tauri::image::Image::from_bytes(tray_icon_for(&core)).expect("tray icon is a valid png");
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("Keepr")
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = ev {
                        toggle_tray_window(tray.app_handle(), rect);
                    }
                })
                .build(app)?;

            if let Some(t) = app.get_webview_window("tray") {
                let t2 = t.clone();
                t.on_window_event(move |ev| {
                    if let tauri::WindowEvent::Focused(false) = ev {
                        let _ = t2.hide();
                    }
                });
            }

            // Screenshot mode's menu-bar scene: the menu-bar window alone, somewhere on screen.
            let tray_scene = scene().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).is_some_and(|v| v["tray"] == true);
            if tray_scene {
                if let Some(t) = app.get_webview_window("tray") {
                    let _ = t.set_position(tauri::LogicalPosition::new(200.0, 120.0));
                    let _ = t.show();
                }
            }
            if let Some(w) = app.get_webview_window("main").filter(|_| !tray_scene) {
                if frozen {
                    let _ = w.set_size(tauri::LogicalSize::new(1440.0, 900.0));
                    let _ = w.center();
                }
                if !quiet {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                // Closing the window hides it; Keepr stays in the menu bar and keeps backing up.
                let w2 = w.clone();
                w.on_window_event(move |ev| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                        api.prevent_close();
                        let _ = w2.app_handle().save_window_state(STATE_FLAGS);
                        let _ = w2.hide();
                        set_in_dock(w2.app_handle(), false);
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, ev| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = ev {
                show_main(app);
            }
            let _ = (app, ev);
        });
}
