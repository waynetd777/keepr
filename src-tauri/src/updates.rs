// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks GitHub for a newer release and offers to install it (tauri-plugin-updater: the
//! endpoint, releases/latest/download/latest.json, and the public key that every package must
//! be signed with are in tauri.conf.json under plugins.updater; tools/update_files.py writes
//! the release's files).
//!
//! When: 20 seconds after launch, then once a day while the app runs (the last check's time is
//! kept in updates.json in the settings folder, so restarting doesn't check again), and whenever
//! the user picks Keepr › Check for Updates… (on Windows, which has no app menu, the button in
//! Settings and the notification-area menu). An update found in the background waits until the
//! main window is next focused, so the offer never pops up over another app. It offers Install
//! and Restart, Later (asked again at the next day's check) or Skip This Version. Release builds
//! only.
//!
//! The offer, the download's progress and the messages ("You're up to date") are the main
//! window's update dialog (src/Update.tsx): sent as `update` events, answered with the
//! `update_install`, `update_later` and `update_skip` commands. Without the window they are the
//! system's alerts.
//!
//! Installing waits while a job is running or queued, and holds the job queue from the moment
//! the download is in until the restart, so the restart can't cut a backup or restore short.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::menu::{MenuItemBuilder, MenuItemKind};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The app menu's Check for Updates… item.
pub const MENU_ID: &str = "app:check-updates";

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const TICK: Duration = Duration::from_secs(60 * 60);

const INSTALL: &str = "Install and Restart";
const LATER: &str = "Later";
const SKIP: &str = "Skip This Version";

/// What the app does around an install.
pub struct Hooks {
    /// Why an update can't be installed right now ("A backup is running."), or None when it can,
    /// in which case nothing new starts until `release`.
    pub hold: fn(&AppHandle) -> Option<String>,
    /// The install failed: let work start again.
    pub release: fn(&AppHandle),
    /// What quitting does, before the restart (which skips it).
    pub quitting: fn(&AppHandle),
}

pub struct Updates {
    /// Found by a background check, offered when the main window is next focused.
    pending: Mutex<Option<Update>>,
    /// Offered in the window, waiting for the answer.
    offered: Mutex<Option<Update>>,
    /// What the window's dialog shows, for a window that loads after it was sent.
    showing: Mutex<Option<Shown>>,
    /// An offer is up, or its install is under way: no second offer until it's over.
    asking: AtomicBool,
    hooks: Hooks,
}

/// The window's update dialog.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Shown {
    Offer { version: String, current: String, notes: Option<String>, date: Option<i64> },
    Progress { version: String, stage: Stage, downloaded: u64, total: Option<u64> },
    Message { title: String, text: String },
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Downloading,
    Installing,
    Restarting,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    checked: u64,
    #[serde(default)]
    skipped: Option<String>,
}

fn saved_path() -> PathBuf {
    crate::config::data_dir().join("updates.json")
}

fn load() -> Saved {
    std::fs::read(saved_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(s: &Saved) {
    let p = saved_path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(b) = serde_json::to_vec_pretty(s) {
        let _ = std::fs::write(p, b);
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn app_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

/// Adds Check for Updates… under About and starts the background checks. Call once from setup
/// (the main thread), in the windowed app only.
pub fn start(app: &AppHandle, hooks: Hooks) -> tauri::Result<()> {
    app.manage(Updates {
        pending: Mutex::new(None),
        offered: Mutex::new(None),
        showing: Mutex::new(None),
        asking: AtomicBool::new(false),
        hooks,
    });
    if let Some(MenuItemKind::Submenu(app_menu)) = app.menu().map(|m| m.items()).transpose()?.and_then(|i| i.into_iter().next()) {
        let at = if app_menu.get(crate::about::ABOUT_ID).is_some() { 1 } else { 0 };
        app_menu.insert(&MenuItemBuilder::with_id(MENU_ID, "Check for Updates…").build(app)?, at)?;
    }
    #[cfg(windows)]
    remove_old_installers(&app_name(app));
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if now().saturating_sub(load().checked) >= EVERY.as_secs() {
                tauri::async_runtime::block_on(check(&app, false));
            }
            std::thread::sleep(TICK);
        }
    });
    Ok(())
}

/// On Windows the updater downloads each update's installer to a folder in Temp
/// ("Keepr-1.1.0-updater-Zu7DmM") and leaves it there after installing. By the time the app starts
/// again that installer has done its work, so the folders go.
#[cfg(windows)]
fn remove_old_installers(name: &str) {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else { return };
    let prefix = format!("{name}-");
    for e in dir.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(&prefix) && n.contains("-updater-") && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Check for Updates…: says what it found, even when there's nothing new.
pub fn check_now(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if cfg!(debug_assertions) {
            tell(&app, "Updates are for release builds", "A development build doesn't check for updates.").await;
        } else {
            check(&app, true).await;
        }
    });
}

/// The main window gained focus: offer an update a background check found.
pub fn focused(app: &AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    let Some(update) = state.pending.lock().unwrap().take() else { return };
    let app = app.clone();
    tauri::async_runtime::spawn(async move { offer(&app, update).await });
}

async fn check(app: &AppHandle, asked: bool) {
    let found = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    let mut saved = load();
    match found {
        Ok(Some(update)) => {
            saved.checked = now();
            save(&saved);
            if asked {
                offer(app, update).await;
            } else if saved.skipped.as_deref() != Some(update.version.as_str()) {
                let focused = app.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
                if focused {
                    offer(app, update).await;
                } else if let Some(state) = app.try_state::<Updates>() {
                    *state.pending.lock().unwrap() = Some(update);
                }
            }
        }
        Ok(None) => {
            saved.checked = now();
            save(&saved);
            if asked {
                let v = app.package_info().version.to_string();
                tell(app, "You're up to date", &format!("{} {v} is the newest version.", app_name(app))).await;
            }
        }
        Err(e) => {
            // A failed background check is tried again within the hour.
            eprintln!("update check failed: {e}");
            if asked {
                // The release has no package for this system (a Windows one comes some minutes
                // after the Mac's, from the release workflow).
                let why = if e.to_string().contains("platform") {
                    "The newest release has no update for this system yet. Try again in a little while.".to_string()
                } else {
                    e.to_string()
                };
                tell(app, "Couldn't check for updates", &why).await;
            }
        }
    }
}

async fn offer(app: &AppHandle, update: Update) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if state.asking.swap(true, Ordering::SeqCst) {
        // Already asked, or installing: the dialog that says so is brought forward.
        if app.get_webview_window("main").is_some() {
            crate::show_main(app);
        }
        return;
    }
    if app.get_webview_window("main").is_none() {
        return offer_natively(app, update).await;
    }
    let shown = Shown::Offer {
        version: update.version.clone(),
        current: update.current_version.clone(),
        notes: notes(&update),
        date: update.date.map(|d| d.unix_timestamp()),
    };
    *state.offered.lock().unwrap() = Some(update);
    show(app, shown);
}

fn notes(update: &Update) -> Option<String> {
    update.body.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(String::from)
}

/// The offer as the system's alert, for when there's no window to show it in.
async fn offer_natively(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    let name = app_name(app);
    let mut text = format!("{name} {} is available. You have {}.", update.version, update.current_version);
    if let Some(n) = notes(&update) {
        text.push_str("\n\n");
        text.push_str(&n);
    }
    let dialog = app
        .dialog()
        .message(text)
        .title(format!("A new version of {name} is available"))
        .buttons(MessageDialogButtons::YesNoCancelCustom(INSTALL.into(), LATER.into(), SKIP.into()));
    let choice =
        tauri::async_runtime::spawn_blocking(move || dialog.blocking_show_with_result()).await.unwrap_or(MessageDialogResult::Cancel);
    // Custom buttons come back as their labels; anything else (Esc, closed) is Later.
    match choice {
        MessageDialogResult::Custom(c) if c == INSTALL => install(app, update).await,
        MessageDialogResult::Custom(c) if c == SKIP => {
            skip(&update);
            state.asking.store(false, Ordering::SeqCst);
        }
        _ => state.asking.store(false, Ordering::SeqCst),
    }
}

fn skip(update: &Update) {
    let mut saved = load();
    saved.skipped = Some(update.version.clone());
    save(&saved);
}

/// Shows the window's update dialog (bringing the window forward for an offer or a message).
fn show(app: &AppHandle, shown: Shown) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if !matches!(shown, Shown::Progress { .. }) {
        crate::show_main(app);
    }
    *state.showing.lock().unwrap() = Some(shown.clone());
    let _ = app.emit_to("main", "update", Some(shown));
}

fn hide(app: &AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    *state.showing.lock().unwrap() = None;
    let _ = app.emit_to("main", "update", None::<Shown>);
}

/// What the update dialog shows now, for a window that has just loaded.
#[tauri::command]
pub fn update_dialog(app: AppHandle) -> Option<Shown> {
    app.try_state::<Updates>()?.showing.lock().unwrap().clone()
}

/// Install and Restart.
#[tauri::command]
pub fn update_install(app: AppHandle) {
    let Some(update) = app.try_state::<Updates>().and_then(|s| s.offered.lock().unwrap().take()) else { return };
    tauri::async_runtime::spawn(async move { install(&app, update).await });
}

/// Later, Esc or closing the offer; OK on a message.
#[tauri::command]
pub fn update_later(app: AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if matches!(*state.showing.lock().unwrap(), Some(Shown::Progress { .. })) {
        return;
    }
    if state.offered.lock().unwrap().take().is_some() {
        state.asking.store(false, Ordering::SeqCst);
    }
    hide(&app);
}

/// Skip This Version.
#[tauri::command]
pub fn update_skip(app: AppHandle) {
    let Some(state) = app.try_state::<Updates>() else { return };
    let Some(update) = state.offered.lock().unwrap().take() else { return };
    skip(&update);
    state.asking.store(false, Ordering::SeqCst);
    hide(&app);
}

async fn install(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    // Whatever ends the install short of the restart lets another offer be made.
    let done = || state.asking.store(false, Ordering::SeqCst);
    let wait = |why: String| format!("{why} {} will offer it again later.", app_name(app));
    // Asked before the download so the answer comes at once, and again after it, holding the
    // queue from there: the download can take a while, and nothing should wait for it.
    if let Some(why) = (state.hooks.hold)(app) {
        done();
        return tell(app, "The update will wait", &wait(why)).await;
    }
    (state.hooks.release)(app);
    let version = update.version.clone();
    let progress = |stage, downloaded, total| {
        if app.get_webview_window("main").is_some() {
            show(app, Shown::Progress { version: version.clone(), stage, downloaded, total });
        }
    };
    progress(Stage::Downloading, 0, None);
    let mut downloaded = 0u64;
    let mut sent = std::time::Instant::now();
    let got = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                // A few times a second is enough for the bar.
                if sent.elapsed() >= Duration::from_millis(100) {
                    sent = std::time::Instant::now();
                    progress(Stage::Downloading, downloaded, total);
                }
            },
            || {},
        )
        .await;
    let bytes = match got {
        Ok(b) => b,
        Err(e) => {
            done();
            return tell(app, "The update didn't download", &e.to_string()).await;
        }
    };
    if let Some(why) = (state.hooks.hold)(app) {
        done();
        return tell(app, "The update will wait", &wait(why)).await;
    }
    let size = bytes.len() as u64;
    progress(Stage::Installing, size, Some(size));
    // Windows' installer closes the app as it starts, so the restart below isn't reached there:
    // what quitting does is done now, and the dialog says it's restarting.
    #[cfg(windows)]
    {
        progress(Stage::Restarting, size, Some(size));
        let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(400))).await;
        (state.hooks.quitting)(app);
    }
    match update.install(bytes) {
        Ok(()) => {
            progress(Stage::Restarting, size, Some(size));
            // Long enough for the dialog to say so.
            let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(400))).await;
            (state.hooks.quitting)(app);
            app.restart()
        }
        Err(e) => {
            (state.hooks.release)(app);
            done();
            tell(app, "The update didn't install", &e.to_string()).await
        }
    }
}

async fn tell(app: &AppHandle, title: &str, text: &str) {
    if app.get_webview_window("main").is_some() && app.try_state::<Updates>().is_some() {
        return show(app, Shown::Message { title: title.into(), text: text.into() });
    }
    let dialog = app.dialog().message(text).title(title).kind(MessageDialogKind::Info);
    let _ = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await;
}
