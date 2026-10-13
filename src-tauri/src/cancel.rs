// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stopping what the window asked for and no longer wants: reading a folder, a size map, the
//! figures for Ask, a search, a Quick Look copy. On a remote destination these read for a while,
//! and leaving the screen alone would let them go on using the network.
//!
//! The window passes a token with the request and calls `cancel` with it (Stop, or leaving the
//! screen). The command runs inside `with`, which makes a Control for the token; the work checks
//! `check()` as it reads listings, or hands `control()` to the engine (a restore), and ends with
//! STOPPED. A cancel that arrives before the work starts stops it as it starts.

use keepr_engine::backup::Control;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};

/// What a stopped request answers; the window, having asked for it, doesn't show it.
pub const STOPPED: &str = "Stopped.";

static LIVE: Mutex<Option<HashMap<String, Arc<Control>>>> = Mutex::new(None);
/// Cancelled before they started (the window gave up while the request was on its way).
static EARLY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

thread_local! {
    static CURRENT: RefCell<Option<Arc<Control>>> = const { RefCell::new(None) };
}

/// Runs `f` as the request `token` names, so it can be cancelled; without a token it can't be.
pub fn with<T>(token: Option<String>, f: impl FnOnce() -> T) -> T {
    let ctl = Arc::new(Control::default());
    if let Some(t) = &token {
        if EARLY.lock().unwrap().get_or_insert_with(HashSet::new).remove(t) {
            ctl.cancel.store(true, Relaxed);
        }
        LIVE.lock().unwrap().get_or_insert_with(HashMap::new).insert(t.clone(), ctl.clone());
    }
    let before = CURRENT.with(|c| c.replace(Some(ctl)));
    let out = f();
    CURRENT.with(|c| *c.borrow_mut() = before);
    if let Some(t) = &token {
        LIVE.lock().unwrap().get_or_insert_with(HashMap::new).remove(t);
    }
    out
}

/// Errs with STOPPED once the request this thread is working on has been cancelled.
pub fn check() -> Result<(), String> {
    if CURRENT.with(|c| c.borrow().as_ref().is_some_and(|ctl| ctl.cancel.load(Relaxed))) {
        Err(STOPPED.into())
    } else {
        Ok(())
    }
}

/// The request's Control, for engine work that takes one (a restore); a fresh one outside `with`.
pub fn control() -> Arc<Control> {
    CURRENT.with(|c| c.borrow().clone()).unwrap_or_default()
}

/// Stops the request `token` names, now or as it starts.
#[tauri::command]
pub fn cancel(token: String) {
    if let Some(ctl) = LIVE.lock().unwrap().get_or_insert_with(HashMap::new).get(&token) {
        ctl.cancel.store(true, Relaxed);
        return;
    }
    let mut early = EARLY.lock().unwrap();
    let early = early.get_or_insert_with(HashSet::new);
    // Tokens for requests that had already finished would otherwise pile up.
    if early.len() > 500 {
        early.clear();
    }
    early.insert(token);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_stops_the_request_it_names_even_before_it_starts() {
        assert!(with(Some("a".into()), check).is_ok());
        cancel("b".into());
        assert_eq!(with(Some("b".into()), check), Err(STOPPED.into()));
        // Once used, the early cancel is gone.
        assert!(with(Some("b".into()), check).is_ok());
        let seen = with(Some("c".into()), || {
            cancel("c".into());
            check()
        });
        assert!(seen.is_err());
        assert!(check().is_ok(), "outside a request nothing is cancelled");
    }
}
