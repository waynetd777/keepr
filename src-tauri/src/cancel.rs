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

/// The requests running, and those cancelled before they started (the window gave up while the
/// request was on its way). One lock for both: with two, a cancel arriving between a request
/// looking for an early cancel and registering as running would find neither and be lost.
#[derive(Default)]
struct Tokens {
    live: HashMap<String, Arc<Control>>,
    early: HashSet<String>,
}

static TOKENS: Mutex<Option<Tokens>> = Mutex::new(None);

thread_local! {
    static CURRENT: RefCell<Option<Arc<Control>>> = const { RefCell::new(None) };
}

/// Runs `f` as the request `token` names, so it can be cancelled; without a token it can't be.
pub fn with<T>(token: Option<String>, f: impl FnOnce() -> T) -> T {
    let ctl = Arc::new(Control::default());
    if let Some(t) = &token {
        let mut g = TOKENS.lock().unwrap();
        let k = g.get_or_insert_with(Tokens::default);
        if k.early.remove(t) {
            ctl.cancel.store(true, Relaxed);
        }
        k.live.insert(t.clone(), ctl.clone());
    }
    let before = CURRENT.with(|c| c.replace(Some(ctl)));
    let out = f();
    CURRENT.with(|c| *c.borrow_mut() = before);
    if let Some(t) = &token {
        TOKENS.lock().unwrap().get_or_insert_with(Tokens::default).live.remove(t);
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
    let mut g = TOKENS.lock().unwrap();
    let k = g.get_or_insert_with(Tokens::default);
    if let Some(ctl) = k.live.get(&token) {
        ctl.cancel.store(true, Relaxed);
        return;
    }
    let early = &mut k.early;
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

    #[test]
    fn a_cancel_made_as_the_request_starts_is_never_lost() {
        // The work waits for the cancel to have returned, then looks: whenever the cancel landed
        // (before the request, or as it registers), the work must see it.
        for i in 0..2000 {
            let t = format!("race-{i}");
            let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (t2, d2) = (t.clone(), done.clone());
            let work = std::thread::spawn(move || {
                with(Some(t2), || {
                    while !d2.load(Relaxed) {
                        std::hint::spin_loop();
                    }
                    check()
                })
            });
            cancel(t);
            done.store(true, Relaxed);
            assert_eq!(work.join().unwrap(), Err(STOPPED.into()), "lost on round {i}");
        }
    }
}
