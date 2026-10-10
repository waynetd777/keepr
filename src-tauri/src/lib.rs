// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keepr's app: the window's commands, the menu-bar item, and starting the job runner and the
//! scheduler. The backup engine itself is the keepr-engine crate (engine/).

mod about;
mod aws_setup;
mod b2_setup;
mod browse;
mod cli;
mod cloud_setup;
mod config;
mod core;
#[cfg(target_os = "macos")]
mod folder_panel;
mod fsevents;
mod help;
mod keychain;
#[cfg(target_os = "macos")]
mod login_item;
#[cfg(target_os = "macos")]
mod login_launch;
mod places;
#[cfg(target_os = "macos")]
mod quick_look;
mod r2_setup;
mod scheduler;
mod smb;
mod still;
mod system;
mod verify;

use crate::config::{Destination, Place, Plan, Run};
use crate::core::{Core, Job, JobStatus};
use serde::Serialize;
use std::sync::Arc;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

type Core_ = Arc<Core>;

/// Runs `f` off the main thread: anything that may wait on a disk, a share, the network or a
/// Keychain prompt. On the main thread it would freeze the window (and the menu bar) until done.
async fn off_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// `off_main`, with the app's Core.
async fn with_core<T: Send + 'static>(core: State<'_, Core_>, f: impl FnOnce(&Core_) -> T + Send + 'static) -> Result<T, String> {
    let core = core.inner().clone();
    off_main(move || f(&core)).await
}

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
        Weekly => format!(
            "{}s at {}",
            ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][p.schedule.weekday as usize % 7],
            p.schedule.at
        ),
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

/// A plan's status when it isn't running: "off", "waiting", "failed", "never", "stale" or "ok",
/// and what to say about it. `last_backup` is its newest backup in the history.
fn status_of(
    p: &Plan,
    ps: &config::PlanState,
    last_backup: Option<&Run>,
    stale_days: u32,
    now: chrono::DateTime<chrono::Local>,
) -> (&'static str, String) {
    let stale = ps
        .last_success
        .as_deref()
        .and_then(keepr_engine::retention::parse_time)
        .is_some_and(|t| now.signed_duration_since(t).num_days() >= stale_days as i64);
    if !p.enabled {
        ("off", "Turned off".to_string())
    } else if let Some(w) = &ps.waiting {
        ("waiting", w.clone())
    } else if let Some(r) = last_backup.filter(|r| r.result == "failed") {
        ("failed", r.message.clone())
    } else if !ps.created || ps.last_success.is_none() {
        ("never", "Hasn't backed up yet".to_string())
    } else if stale {
        ("stale", "No backup for a while".to_string())
    } else {
        ("ok", last_backup.map(|r| r.message.clone()).unwrap_or_default())
    }
}

fn is_backup(r: &Run) -> bool {
    r.kind == "backup" || r.kind == "full"
}

fn overview_of(core: &Core) -> Overview {
    let cfg = core.config.lock().unwrap().clone();
    let st = core.state.lock().unwrap().clone();
    let job = core.status();
    let now = chrono::Local::now();
    let paused_until = st.paused_until.as_deref().and_then(keepr_engine::retention::parse_time).filter(|t| *t > now);
    let mut plans = Vec::new();
    for p in &cfg.plans {
        let ps = st.plans.get(&p.id).cloned().unwrap_or_default();
        let runs: Vec<&Run> = st.history.iter().filter(|r| r.plan == p.id).collect();
        let last_backup = runs.iter().rev().find(|r| is_backup(r));
        let running = job.as_ref().is_some_and(|j| j.plan == p.id);
        let (status, message) = if running && p.enabled {
            ("running", job.as_ref().map(|j| j.stage.clone()).unwrap_or_default())
        } else {
            status_of(p, &ps, last_backup.copied(), cfg.settings.stale_days, now)
        };
        let mut days: Vec<Day> = (0..30).map(|_| Day { added: 0, failed: false, ran: false, count: 0 }).collect();
        for r in &runs {
            if !is_backup(r) {
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
            schedule: schedule_label(p),
            encrypted: p.encrypted,
            status: status.into(),
            message,
            last_success: ps.last_success.clone(),
            last_attempt: ps.last_attempt.clone(),
            next_run: if p.enabled {
                Core::next_run(p, ps.last_attempt.as_deref()).map(|t| paused_until.map_or(t, |u| t.max(u)).to_rfc3339())
            } else {
                None
            },
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

/// Off the main thread: it looks at each destination, and a share that has gone away can take a
/// long time to say so.
#[tauri::command]
async fn overview(core: State<'_, Core_>) -> Result<Overview, String> {
    with_core(core, |c| overview_of(c)).await
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
    std::fs::read_to_string(core.dir.join("logs").join(format!("{id}.log")))
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
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

/// Off the main thread, like every command that uses the Keychain: it may ask the user first.
#[tauri::command]
async fn save_plan(core: State<'_, Core_>, plan: Plan, password: Option<String>) -> Result<Plan, String> {
    with_core(core, move |c| save_plan_now(c, plan, password)).await?
}

fn save_plan_now(core: &Core, mut plan: Plan, password: Option<String>) -> Result<Plan, String> {
    if plan.name.trim().is_empty() {
        return Err("Give the plan a name.".into());
    }
    if plan.sources.is_empty() {
        return Err("Add at least one folder to back up.".into());
    }
    if let Some(why) = config::overlapping_sources(&plan.sources) {
        return Err(why);
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
            // The folder only changes through rename_plan_folder: a page still holding the old
            // name must not point the plan back at a folder that has gone.
            plan.folder = old.folder.clone();
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

/// What a plan's backup folder would be called for this name.
#[tauri::command]
fn suggest_plan_folder(name: String, id: String) -> String {
    config::folder_name(&name, &id)
}

/// Renames a plan's backup folder to match its name.
#[tauri::command]
async fn rename_plan_folder(core: State<'_, Core_>, id: String) -> Result<String, String> {
    with_core(core, move |c| c.rename_plan_folder(&id)).await?
}

/// Puts the plans in this order (dragged on Backup Plans); the order is used wherever plans are listed.
#[tauri::command]
fn reorder_plans(core: State<Core_>, ids: Vec<String>) -> Result<(), String> {
    {
        let mut c = core.config.lock().unwrap();
        let rank = |id: &str| ids.iter().position(|x| x == id).unwrap_or(usize::MAX);
        // Stable: a plan the list doesn't name keeps its place after the named ones.
        c.plans.sort_by_key(|p| rank(&p.id));
    }
    core.save_config()?;
    core.changed();
    Ok(())
}

#[tauri::command]
async fn delete_plan(core: State<'_, Core_>, id: String) -> Result<(), String> {
    with_core(core, move |core| {
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
    })
    .await?
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
async fn save_destination(core: State<'_, Core_>, dest: Destination, password: Option<String>) -> Result<Destination, String> {
    with_core(core, move |c| save_destination_now(c, dest, password)).await?
}

fn save_destination_now(core: &Core, mut dest: Destination, password: Option<String>) -> Result<Destination, String> {
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
        (Place::S3(s), None) if keychain::get(&keychain::s3_account(&s.access_key)).is_none() => {
            return Err("Enter the bucket's secret key.".into())
        }
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
    let taken: Vec<String> =
        core.config.lock().unwrap().destinations.iter().filter(|d| Some(&d.id) != except.as_ref()).map(|d| d.name.clone()).collect();
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
                Ok(mbps) => Tested {
                    ok: true,
                    message: "Connected, and Keepr can write to this bucket.".into(),
                    free: None,
                    total: None,
                    mbps: Some(mbps),
                },
                Err(e) => fail(e),
            };
        }
        // Through resolve, so a share this connects is noted and disconnected again when idle.
        let base = match places::resolve_with(&place, &core.mounts, true, true, password.as_deref().filter(|p| !p.is_empty())) {
            Ok(p) => p,
            Err(e) => return fail(e),
        };
        if let Err(e) = std::fs::create_dir_all(&base) {
            return fail(format!("Keepr can't make its folder there: {e}"));
        }
        let probe = base.join(format!(".keepr-test-{}", config::new_id()));
        let data = vec![0x5au8; 8 << 20];
        let t = std::time::Instant::now();
        let res =
            std::fs::write(&probe, &data).and_then(|_| keepr_engine::backend::sync(&std::fs::OpenOptions::new().write(true).open(&probe)?));
        let secs = t.elapsed().as_secs_f64();
        let _ = std::fs::remove_file(&probe);
        if let Err(e) = res {
            return fail(format!("Keepr can't write there: {e}"));
        }
        let space = places::space(&base);
        Tested {
            ok: true,
            message: "Connected, and Keepr can write here.".into(),
            free: space.map(|s| s.0),
            total: space.map(|s| s.1),
            mbps: Some(8.0 * 1.048_576 / secs.max(0.001)),
        }
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
    let cli = off_main(|| aws_setup::cli().is_some()).await.unwrap_or(false);
    AwsSetupInfo { cli, bucket: aws_setup::suggest_bucket() }
}

/// Signs in through the browser and makes the bucket and its user.
#[tauri::command]
async fn aws_setup_run(region: String, bucket: String, mode: aws_setup::Mode) -> Result<aws_setup::Made, String> {
    off_main(move || aws_setup::with_cli(region.trim(), bucket.trim(), mode)).await?
}

/// Backblaze B2: the buckets a master key can see.
#[tauri::command]
async fn b2_buckets(key_id: String, key: String) -> Result<Vec<String>, String> {
    off_main(move || b2_setup::list(&key_id, &key)).await?
}

/// Backblaze B2: makes (or finds) the bucket and a key for it alone, from a master key that
/// isn't kept.
#[tauri::command]
async fn b2_setup_run(key_id: String, key: String, bucket: String, mode: aws_setup::Mode) -> Result<aws_setup::Made, String> {
    off_main(move || b2_setup::setup(&key_id, &key, &bucket, mode)).await?
}

/// Cloudflare R2: the buckets a setup token's account has.
#[tauri::command]
async fn r2_buckets(token: String, account: String) -> Result<Vec<String>, String> {
    off_main(move || r2_setup::list(&token, &account)).await?
}

/// Cloudflare R2: makes (or finds) the bucket and S3 keys for it alone, then deletes the setup token.
#[tauri::command]
async fn r2_setup_run(token: String, account: String, bucket: String, mode: aws_setup::Mode) -> Result<aws_setup::Made, String> {
    off_main(move || r2_setup::setup(&token, &account, &bucket, mode)).await?
}

/// Signs in to AWS through the browser and lists the buckets there.
#[tauri::command]
async fn aws_buckets(region: String) -> Result<Vec<String>, String> {
    off_main(move || aws_setup::buckets(region.trim())).await?
}

/// Forgets a sign-in kept for choosing a bucket (the sheet was closed).
#[tauri::command]
async fn aws_setup_end() {
    let _ = off_main(aws_setup::end_session).await;
}

/// Checks a bucket as a source: that the key may list it, and how much is in it. Saves the
/// secret once it works.
#[tauri::command]
async fn check_s3_source(place: Place, secret: Option<String>) -> Result<String, String> {
    off_main(move || {
        let Place::S3(s) = &place else { return Err("That isn't a bucket.".to_string()) };
        let b = places::s3_backend(s, secret.clone(), "")?;
        b.check_bucket().map_err(|e| e.to_string())?;
        let objects = keepr_engine::backup::Remote::objects(&b).map_err(|e| e.to_string())?;
        if let Some(pw) = secret.filter(|p| !p.is_empty()) {
            keychain::set(&keychain::s3_account(&s.access_key), &pw)?;
        }
        let bytes: u64 = objects.iter().map(|o| o.size).sum();
        Ok(format!("{} files, {}", objects.len(), core::human_bytes(bytes)))
    })
    .await?
}

/// The same setup as a script to paste into AWS CloudShell.
#[tauri::command]
fn aws_setup_script(region: String, bucket: String, mode: aws_setup::Mode) -> Result<String, String> {
    aws_setup::script(region.trim(), bucket.trim(), mode)
}

/// The line CloudShell printed at the end.
#[tauri::command]
async fn aws_setup_paste(text: String) -> Result<aws_setup::Made, String> {
    off_main(move || aws_setup::parse(&text).and_then(aws_setup::finish)).await?
}

/// The cloud services' sync folders on this Mac, for Add a destination.
#[tauri::command]
async fn cloud_folders() -> Vec<places::CloudFolder> {
    off_main(places::cloud_folders).await.unwrap_or_default()
}

/// SMB servers to offer: ones Keepr already uses, ones mounted now, and ones on Bonjour.
#[tauri::command]
async fn discover_servers(core: State<'_, Core_>) -> Result<Vec<String>, String> {
    let mut known: Vec<String> = {
        let c = core.config.lock().unwrap();
        c.destinations
            .iter()
            .map(|d| &d.place)
            .chain(c.plans.iter().flat_map(|p| p.sources.iter()))
            .filter_map(|p| if let Place::Smb(s) = p { Some(s.server.clone()) } else { None })
            .collect()
    };
    known.extend(smb::mounted_servers());
    let found = off_main(smb::discover).await.unwrap_or_default();
    known.extend(found.into_iter().map(|n| format!("{n}.local")));
    let mut seen = std::collections::HashSet::new();
    known.retain(|s| seen.insert(s.to_lowercase()));
    Ok(known)
}

#[tauri::command]
async fn list_shares(server: String, user: String, password: Option<String>) -> Result<Vec<String>, String> {
    off_main(move || {
        let pw = password
            .filter(|p| !p.is_empty())
            .or_else(|| keychain::get(&keychain::smb_account(&user, &server)))
            .or_else(|| keychain::finder_smb_password(&server, &user))
            .unwrap_or_default();
        smb::shares(&server, &user, &pw)
    })
    .await?
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
    off_main(move || keychain::saved_smb_user(&server).map(|(user, source)| SavedLogin { user, source: source.into() }))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
async fn save_smb_password(server: String, user: String, password: String) -> Result<(), String> {
    off_main(move || keychain::set(&keychain::smb_account(&user, &server), &password)).await?
}

#[tauri::command]
async fn recovery_key(id: String) -> Result<Option<String>, String> {
    off_main(move || keychain::get(&keychain::recovery_account(&id))).await
}

#[tauri::command]
fn recovery_saved(core: State<Core_>, id: String) {
    core.state.lock().unwrap().plan(&id).recovery_unsaved = false;
    core.save_state();
    core.changed();
}

/// What the user typed as the current password: a recovery key (long, in dashed groups) or the
/// password itself.
fn secret_of(typed: &str) -> keepr_engine::repo::Secret<'_> {
    if typed.contains('-') && typed.len() > 40 {
        keepr_engine::repo::Secret::RecoveryKey(typed)
    } else {
        keepr_engine::repo::Secret::Password(typed)
    }
}

#[tauri::command]
async fn change_password(core: State<'_, Core_>, id: String, current: String, new: String) -> Result<(), String> {
    with_core(core, move |core| {
        if new.len() < 8 {
            return Err("Use at least 8 characters.".to_string());
        }
        // A backup running meanwhile would keep the repository opened with the old key.
        if core.busy_with(&id) {
            return Err("Wait until this plan's backup has finished.".into());
        }
        let repo = core.repo(&id, false)?;
        let backend = repo.backend.clone();
        drop(repo);
        core.forget_repo(&id);
        let mut r = keepr_engine::repo::Repo::open(backend, secret_of(&current)).map_err(|e| e.0)?;
        r.change_password(secret_of(&current), &new).map_err(|e| e.0)?;
        keychain::set(&keychain::plan_account(&id), &new)
    })
    .await?
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
fn restore(
    core: State<Core_>,
    plan: String,
    snapshot: String,
    items: Vec<String>,
    target: keepr_engine::restore::Target,
    conflict: keepr_engine::restore::Conflict,
) -> String {
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
fn job_cancel(core: State<Core_>, id: String) {
    core.cancel(&id)
}

#[tauri::command]
fn job_pause(core: State<Core_>, paused: bool) {
    core.pause(paused)
}

/// Pauses scheduled backups for an hour, and stops the backup that's running (a restore or
/// check carries on). The plans' last attempts stay as they were.
#[tauri::command]
fn pause_hour(core: State<Core_>) {
    let later = (chrono::Local::now() + chrono::Duration::minutes(60)).to_rfc3339();
    core.state.lock().unwrap().paused_until = Some(later);
    core.save_state();
    core.cancel_backup();
    core.changed();
}

// ---- restore browsing ----

#[tauri::command]
async fn snapshots(core: State<'_, Core_>, plan: String) -> Result<Vec<browse::SnapInfo>, String> {
    with_core(core, move |c| Ok(c.snapshots(&plan)?.iter().map(browse::info).collect())).await?
}

/// Show in Finder: the backed-up item where it is on the Mac now, its SMB share connected first.
#[tauri::command]
async fn show_in_finder(core: State<'_, Core_>, plan: String, path: String) -> Result<(), String> {
    let at = with_core(core, move |c| browse::on_mac(c, &plan, &path)).await??;
    tauri_plugin_opener::reveal_item_in_dir(at).map_err(|e| e.to_string())
}

#[tauri::command]
async fn size_map(core: State<'_, Core_>, plan: String, snapshot: String, path: String) -> Result<browse::MapDir, String> {
    with_core(core, move |c| browse::size_map(c, &plan, &snapshot, &path)).await?
}

#[tauri::command]
async fn list_dir(
    core: State<'_, Core_>,
    plan: String,
    snapshot: String,
    path: String,
    show_deleted: bool,
) -> Result<Vec<browse::Entry>, String> {
    with_core(core, move |c| browse::list(c, &plan, &snapshot, &path, show_deleted)).await?
}

#[tauri::command]
async fn file_versions(core: State<'_, Core_>, plan: String, path: String) -> Result<Vec<browse::VersionInfo>, String> {
    with_core(core, move |c| browse::versions(c, &plan, &path)).await?
}

#[tauri::command]
async fn search_snapshot(core: State<'_, Core_>, plan: String, snapshot: String, query: String) -> Result<Vec<browse::Entry>, String> {
    with_core(core, move |c| browse::search(c, &plan, &snapshot, &query)).await?
}

#[tauri::command]
async fn compare(core: State<'_, Core_>, plan: String, snapshot: String, path: String) -> Result<browse::Comparison, String> {
    with_core(core, move |c| browse::compare(c, &plan, &snapshot, &path)).await?
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
    with_core(core, move |c| {
        let (found, missed) = browse::search_everywhere(c, &query);
        Everywhere { found, missed }
    })
    .await
}

#[tauri::command]
async fn quick_look(app: AppHandle, core: State<'_, Core_>, plan: String, snapshot: String, path: String) -> Result<(), String> {
    let file = with_core(core, move |c| browse::preview_copy(c, &plan, &snapshot, &path)).await??;
    #[cfg(target_os = "macos")]
    {
        let window = app.get_webview_window("main").ok_or("No window to show Quick Look over.")?;
        let ns = window.ns_window().map_err(|e| e.to_string())? as usize;
        app.run_on_main_thread(move || {
            let mtm = objc2::MainThreadMarker::new().expect("on the main thread");
            // Tauri's own NSWindow, alive for as long as the app is.
            let window = unsafe { &*(ns as *const objc2_app_kit::NSWindow) };
            quick_look::show(mtm, window, &file);
        })
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, file);
    Ok(())
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
        off_main(move || rx.recv().unwrap_or_default()).await.unwrap_or_default()
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
    // Screenshots run the unbundled dev build, which can't register; show it as the installed app would.
    if scene().is_some() {
        return (true, true);
    }
    #[cfg(target_os = "macos")]
    {
        (login_item::available(), login_item::status().is_on())
    }
    #[cfg(not(target_os = "macos"))]
    (false, false)
}

#[tauri::command]
fn set_login_item(on: bool) -> bool {
    if scene().is_some() {
        return on;
    }
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

/// Makes a window invisible and click-through while it still draws, for screenshots
/// (tools/screenshots.py saves its webview's snapshot): nothing flashes on screen. macOS only.
fn make_unseen(w: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ns) = w.ns_window() {
        // Tauri's own NSWindow, alive as long as the window is; setup runs on the main thread.
        let window = unsafe { &*(ns as *const objc2_app_kit::NSWindow) };
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = w;
}

/// Saves what the window's webview shows to `dest` as a TIFF, for screenshots taken unseen
/// (make_unseen: an invisible window's own capture is blank, its webview's snapshot isn't). The
/// file appears, written whole, once WebKit has drawn it. macOS only.
fn snapshot(w: &tauri::WebviewWindow, dest: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let dest = dest.to_path_buf();
        w.with_webview(move |wv| unsafe {
            use objc2::runtime::AnyObject;
            let webview = wv.inner() as *mut AnyObject;
            let done = block2::RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
                if image.is_null() {
                    return;
                }
                let tiff: *mut AnyObject = objc2::msg_send![image, TIFFRepresentation];
                if !tiff.is_null() {
                    let path = objc2_foundation::NSString::from_str(&dest.to_string_lossy());
                    let _: bool = objc2::msg_send![tiff, writeToFile: &*path, atomically: true];
                }
            });
            let config: *mut AnyObject = std::ptr::null_mut();
            let _: () = objc2::msg_send![webview, takeSnapshotWithConfiguration: config, completionHandler: &*done];
        })
        .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (w, dest);
        Err("Snapshots are macOS only.".into())
    }
}

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
/// From what Keepr already knows, never the disks: it's worked out on the main thread at every
/// change, and a share that has gone away would freeze the app.
fn tray_icon_for(core: &Core) -> &'static [u8] {
    let attention = || {
        let cfg = core.config.lock().unwrap();
        let st = core.state.lock().unwrap();
        let now = chrono::Local::now();
        cfg.plans.iter().any(|p| {
            let ps = st.plans.get(&p.id).cloned().unwrap_or_default();
            let last = st.history.iter().rev().find(|r| r.plan == p.id && is_backup(r));
            matches!(status_of(p, &ps, last, cfg.settings.stale_days, now).0, "failed" | "waiting" | "stale")
        })
    };
    if core.running() {
        include_bytes!("../icons/tray-busy@2x.png")
    } else if attention() {
        include_bytes!("../icons/tray-alert@2x.png")
    } else {
        include_bytes!("../icons/tray@2x.png")
    }
}

/// When the menu window last closed itself on losing focus.
static TRAY_BLURRED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// A click on the icon this soon after the menu closed on losing focus is the click that took the
/// focus: it closes the menu rather than opening it again.
const CLICK_AFTER_BLUR: std::time::Duration = std::time::Duration::from_millis(400);

/// Where the icon was that the menu last opened under. With two screens each menu bar has the
/// icon, and a click on the other screen's takes the focus too: that one opens the menu there.
static OPENED_UNDER: std::sync::Mutex<Option<(f64, f64)>> = std::sync::Mutex::new(None);

/// The icon's place as the click gives it, to tell one screen's icon from another's.
fn icon_place(rect: &tauri::Rect) -> (f64, f64) {
    let p = rect.position.to_physical::<f64>(1.0);
    (p.x, p.y)
}

fn toggle_tray_window(app: &AppHandle, rect: tauri::Rect) {
    let Some(w) = app.get_webview_window("tray") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    // With the main window closed, clicking the icon takes the focus from the menu first, so it has
    // just hidden itself: this click was meant to close it.
    let same_icon = *OPENED_UNDER.lock().unwrap() == Some(icon_place(&rect));
    if TRAY_BLURRED_AT.lock().unwrap().take().is_some_and(|t| t.elapsed() < CLICK_AFTER_BLUR) && same_icon {
        return;
    }
    *OPENED_UNDER.lock().unwrap() = Some(icon_place(&rect));
    // Placed in points: on macOS they're one space across screens, where physical pixels aren't
    // (each screen's are its points times its own scale, so with a Retina screen beside another,
    // a position in pixels lands on the wrong screen or in the wrong place). The click gives the
    // icon in the pixels of the screen it's on; the screen it's on is the one where, in that
    // screen's points, it falls inside it. The icon can be reported a few points above its
    // screen's top edge (seen: 5 on a screen placed higher than the built-in one), so it counts
    // as on the screen within an icon's height of its top; across, it must be inside.
    let monitors = app.available_monitors().unwrap_or_default();
    let icon = rect.position.to_physical::<f64>(1.0);
    let icon_size = rect.size.to_physical::<f64>(1.0);
    let in_points = |m: &tauri::Monitor| {
        let k = m.scale_factor();
        let (p, sz) = (m.position(), m.size());
        let (left, top) = (p.x as f64 / k, p.y as f64 / k);
        let (x, y, slack) = (icon.x / k, icon.y / k, icon_size.height / k);
        let inside = x >= left && x < left + sz.width as f64 / k && y >= top - slack && y < top + sz.height as f64 / k;
        inside.then_some((k, left, left + sz.width as f64 / k))
    };
    let screen = monitors.iter().find_map(in_points);
    let k = screen.map_or_else(|| w.scale_factor().unwrap_or(2.0), |s| s.0);
    let (x, y, iw, ih) = (icon.x / k, icon.y / k, icon_size.width / k, icon_size.height / k);
    let mut left = x + iw / 2.0 - 360.0 / 2.0;
    if let Some((_, screen_left, screen_right)) = screen {
        left = left.min(screen_right - 360.0 - 8.0).max(screen_left + 8.0);
    }
    let place = tauri::LogicalPosition::new(left, y + ih + 6.0);
    let _ = w.set_position(place);
    let _ = w.show();
    // Once on that screen, place it again, in case macOS moved it while showing it.
    let _ = w.set_position(place);
    let _ = w.set_focus();
    let _ = w.emit("tray-opened", ());
}

/// `keepr <command>` (see cli.rs): Some(exit status) when the arguments are for the command
/// line, None to start the app.
pub fn cli(args: &[String]) -> Option<i32> {
    cli::wanted(args).then(|| cli::main(args))
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
            reorder_plans,
            suggest_plan_folder,
            rename_plan_folder,
            aws_setup_run,
            aws_setup_script,
            aws_setup_paste,
            aws_buckets,
            b2_buckets,
            b2_setup_run,
            r2_buckets,
            r2_setup_run,
            aws_setup_end,
            check_s3_source,
            get_config,
            history,
            run_log,
            default_excludes,
            save_plan,
            delete_plan,
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
            recovery_key,
            recovery_saved,
            change_password,
            back_up,
            back_up_all,
            restore,
            check_now,
            remove_source_data,
            remove_path_data,
            job_cancel,
            job_pause,
            pause_hour,
            snapshots,
            list_dir,
            size_map,
            show_in_finder,
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
        .on_menu_event(|app, ev| {
            if ev.id() == help::MENU_ID {
                help::show_drawer(app);
            } else if ev.id() == about::WEBSITE_ID {
                about::open_website(app);
            } else if ev.id() == about::ABOUT_ID {
                about::show(app);
            }
        })
        .setup(move |app| {
            help::add_to_menu(app.handle())?;
            std::thread::spawn(|| (aws_setup::sweep(), browse::clear_previews()));
            about::set_about_item(app.handle())?;
            if !frozen {
                std::thread::spawn(keychain::warm);
            }
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

            // Held until the app ends, so the command-line jobs know it's running (see cli_core).
            // Without it (a command-line backup holds it now) the app runs as before.
            if !frozen {
                match config::lock_data_dir(&config::data_dir(), false) {
                    Ok(lock) => std::mem::forget(lock),
                    Err(e) => eprintln!("data folder lock: {e}"),
                }
            }

            let handle = app.handle().clone();
            let emitter = handle.clone();
            let notifier = handle.clone();
            let core = match Core::new(
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
            ) {
                Ok(c) => c,
                Err(e) => {
                    // Nothing is shown and nothing runs: say why, and end once it's read.
                    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                    let h = app.handle().clone();
                    app.dialog()
                        .message(format!(
                            "{e}\n\nKeepr has left its settings and history as they are. Check the disk, then open Keepr again."
                        ))
                        .title("Keepr can't start")
                        .kind(MessageDialogKind::Error)
                        .show(move |_| h.exit(1));
                    return Ok(());
                }
            };
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
                        if t2.is_visible().unwrap_or(false) {
                            *TRAY_BLURRED_AT.lock().unwrap() = Some(std::time::Instant::now());
                        }
                        let _ = t2.hide();
                    }
                });
            }

            // Screenshot mode's menu-bar scene: the menu-bar window alone, somewhere on screen.
            let tray_scene = scene().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).is_some_and(|v| v["tray"] == true);
            if tray_scene {
                if let Some(t) = app.get_webview_window("tray") {
                    let _ = t.set_position(tauri::LogicalPosition::new(200.0, 120.0));
                    make_unseen(&t);
                    let _ = t.show();
                }
            }
            // tools/screenshots.py: the scene's snapshot to KEEPR_SNAPSHOT, once it has settled.
            if let (true, Some(out)) = (frozen, std::env::var_os("KEEPR_SNAPSHOT")) {
                let label = if tray_scene { "tray" } else { "main" };
                let after = std::env::var("KEEPR_SNAPSHOT_AFTER").ok().and_then(|s| s.parse().ok()).unwrap_or(5.0);
                let app = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs_f64(after));
                    if let Some(w) = app.get_webview_window(label) {
                        let _ = snapshot(&w, std::path::Path::new(&out));
                    }
                });
            }
            if let Some(w) = app.get_webview_window("main").filter(|_| !tray_scene) {
                // Screenshots are taken at one size, unseen and out of the Dock, without taking the focus.
                if frozen {
                    let _ = w.set_size(tauri::LogicalSize::new(1440.0, 900.0));
                    let _ = w.center();
                    make_unseen(&w);
                    set_in_dock(app.handle(), false);
                }
                if !quiet {
                    let _ = w.show();
                    if !frozen {
                        let _ = w.set_focus();
                    }
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
                // Not when Keepr couldn't start (its message is up): the window would have no Core.
                if app.try_state::<Core_>().is_some() {
                    show_main(app);
                }
            }
            let _ = (app, ev);
        });
}
