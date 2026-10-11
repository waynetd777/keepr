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
//! the user picks Keepr › Check for Updates…. An update found in the background waits until the
//! main window is next focused, so the offer never pops up over another app. It offers Install
//! and Restart, Later (asked again at the next day's check) or Skip This Version. Release builds
//! only.
//!
//! Installing waits while a job is running or queued, and holds the job queue from the moment
//! the download is in until the restart, so the restart can't cut a backup or restore short.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::menu::{MenuItemBuilder, MenuItemKind};
use tauri::{AppHandle, Manager};
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
    pending: Mutex<Option<Update>>,
    asking: AtomicBool,
    hooks: Hooks,
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
    app.manage(Updates { pending: Mutex::new(None), asking: AtomicBool::new(false), hooks });
    if let Some(MenuItemKind::Submenu(app_menu)) = app.menu().map(|m| m.items()).transpose()?.and_then(|i| i.into_iter().next()) {
        let at = if app_menu.get(crate::about::ABOUT_ID).is_some() { 1 } else { 0 };
        app_menu.insert(&MenuItemBuilder::with_id(MENU_ID, "Check for Updates…").build(app)?, at)?;
    }
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
                tell(app, "Couldn't check for updates", &e.to_string()).await;
            }
        }
    }
}

async fn offer(app: &AppHandle, update: Update) {
    let Some(state) = app.try_state::<Updates>() else { return };
    if state.asking.swap(true, Ordering::SeqCst) {
        return;
    }
    let name = app_name(app);
    let mut text = format!("{name} {} is available. You have {}.", update.version, update.current_version);
    if let Some(notes) = update.body.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        text.push_str("\n\n");
        text.push_str(notes);
    }
    let dialog = app
        .dialog()
        .message(text)
        .title(format!("A new version of {name} is available"))
        .buttons(MessageDialogButtons::YesNoCancelCustom(INSTALL.into(), LATER.into(), SKIP.into()));
    let choice =
        tauri::async_runtime::spawn_blocking(move || dialog.blocking_show_with_result()).await.unwrap_or(MessageDialogResult::Cancel);
    state.asking.store(false, Ordering::SeqCst);
    // Custom buttons come back as their labels; anything else (Esc, closed) is Later.
    match choice {
        MessageDialogResult::Custom(c) if c == INSTALL => install(app, update).await,
        MessageDialogResult::Custom(c) if c == SKIP => {
            let mut saved = load();
            saved.skipped = Some(update.version.clone());
            save(&saved);
        }
        _ => {}
    }
}

async fn install(app: &AppHandle, update: Update) {
    let state = app.state::<Updates>();
    let wait = |why: String| format!("{why} {} will offer it again later.", app_name(app));
    // Asked before the download so the answer comes at once, and again after it, holding the
    // queue from there: the download can take a while, and nothing should wait for it.
    if let Some(why) = (state.hooks.hold)(app) {
        return tell(app, "The update will wait", &wait(why)).await;
    }
    (state.hooks.release)(app);
    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(b) => b,
        Err(e) => return tell(app, "The update didn't download", &e.to_string()).await,
    };
    if let Some(why) = (state.hooks.hold)(app) {
        return tell(app, "The update will wait", &wait(why)).await;
    }
    match update.install(bytes) {
        Ok(()) => {
            (state.hooks.quitting)(app);
            app.restart()
        }
        Err(e) => {
            (state.hooks.release)(app);
            tell(app, "The update didn't install", &e.to_string()).await
        }
    }
}

async fn tell(app: &AppHandle, title: &str, text: &str) {
    let dialog = app.dialog().message(text).title(title).kind(MessageDialogKind::Info);
    let _ = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show()).await;
}
