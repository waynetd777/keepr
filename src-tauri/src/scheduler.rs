// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starting backups when they're due, twice a minute. Timed here rather than in the page, which
//! is throttled once the window is closed.
//!
//! A backup waits (without an entry in the history each time) while its destination isn't
//! connected, the battery is low, or the network is a personal hotspot, and runs as soon as that
//! changes. A plan that hasn't completed a backup for the settings' number of days gets one
//! notification a day saying so: a backup that has quietly stopped is the failure to fear most.

use crate::config::Every;
use crate::core::{Core, Job};
use crate::system;
use chrono::Local;
use keepr_engine::retention::parse_time;
use std::sync::Arc;
use std::time::Duration;

/// Why a due backup should wait, if it should.
fn blocked(c: &crate::config::Conditions) -> Option<String> {
    if !c.on_battery || c.min_battery > 0 {
        if let Some(pct) = system::battery_percent() {
            if !c.on_battery {
                return Some("Waiting for power (on battery)".into());
            }
            if pct < c.min_battery {
                return Some(format!("Waiting for power (battery at {pct}%)"));
            }
        }
    }
    if c.no_hotspot && system::on_expensive_network() {
        return Some("Waiting: on a personal hotspot".into());
    }
    None
}

pub fn tick(core: &Arc<Core>) {
    let (plans, settings) = {
        let c = core.config.lock().unwrap();
        (c.plans.clone(), c.settings.clone())
    };
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    // Pause for an hour holds back scheduled backups only; it doesn't move the plans' own times.
    let paused = core.state.lock().unwrap().paused_until.as_deref().and_then(parse_time).is_some_and(|t| t > now);
    for plan in plans.iter().filter(|p| p.enabled) {
        if core.busy_with(&plan.id) {
            continue;
        }
        let ps = core.state.lock().unwrap().plans.get(&plan.id).cloned().unwrap_or_default();

        // Stale: warn once a day.
        let since = ps.last_success.as_deref().or(ps.last_attempt.as_deref()).and_then(parse_time);
        if plan.schedule.every != Every::Manual && ps.created {
            if let Some(t) = since {
                let days = now.signed_duration_since(t).num_days();
                if days >= settings.stale_days as i64 && ps.stale_warned.as_deref() != Some(&today) {
                    let when = t.format("%A %-d %B").to_string();
                    (core.notify)(
                        &format!("{} hasn't backed up since {when}", plan.name),
                        ps.waiting.as_deref().unwrap_or("Open Keepr to see why."),
                    );
                    core.state.lock().unwrap().plan(&plan.id).stale_warned = Some(today.clone());
                    core.save_state();
                }
            }
        }

        if plan.schedule.every == Every::Manual || paused {
            continue;
        }
        // Waiting (for a destination, power or another network): look again every five minutes.
        let due = if ps.waiting.is_some() {
            ps.last_attempt.as_deref().and_then(parse_time).is_none_or(|t| now.signed_duration_since(t).num_minutes() >= 5)
        } else {
            let Some(next) = Core::next_run(plan, ps.last_attempt.as_deref()) else { continue };
            if next > now {
                continue;
            }
            // Missed while asleep or off, and not to be caught up: move on to the next slot.
            if !plan.conditions.catch_up && now.signed_duration_since(next).num_minutes() > 15 && ps.last_attempt.is_some() {
                core.state.lock().unwrap().plan(&plan.id).last_attempt = Some(now.to_rfc3339());
                core.save_state();
                continue;
            }
            true
        };
        if !due {
            continue;
        }
        if let Some(why) = blocked(&plan.conditions) {
            let mut s = core.state.lock().unwrap();
            if s.plan(&plan.id).waiting.as_deref() != Some(&why) {
                s.plan(&plan.id).waiting = Some(why);
                drop(s);
                core.save_state();
                core.changed();
            }
            continue;
        }
        let full = Core::full_due(plan, ps.last_full.as_deref()) && ps.created;
        core.enqueue(Job::Backup { plan: plan.id.clone(), full });
    }
    if !core.running() {
        core.mounts.reap(Duration::from_secs(5 * 60));
    }
}

pub fn start(core: Arc<Core>) {
    // KEEPR_NO_SCHEDULE: a second Keepr (a dev build beside a command-line backup) that mustn't
    // start backups of its own.
    if core.frozen || std::env::var_os("KEEPR_NO_SCHEDULE").is_some() {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        loop {
            tick(&core);
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}
