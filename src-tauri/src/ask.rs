// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ask's side of Keepr (the AI itself runs in assistant/): what a question carries, and the plan
//! changes an answer proposes.
//!
//! The context is Keepr's own figures, as the help's Ask page (src/help/ask.md) lists them:
//! each plan's settings and status, its biggest folders and what grew in them, the space they take
//! stored, folder names that repeat (42 node_modules folders), and the recent runs. Folders are
//! named; files never are, and nothing of their contents goes. A plan left out of Ask is named
//! and nothing else. Each listed folder carries facts that say what it is without file names:
//! its file types by size, how recently its files changed, and whether its project's .gitignore
//! lists it.
//!
//! A proposal is data, never a command: `check` works out what it would do (the diff, what the
//! latest backup has that would be left out, the snapshots a tidy-up would remove) and what to
//! warn about; `apply` saves it through the plan editor's own path and records it in Activity,
//! where `undo` puts back what it changed. Only the fields in ALLOWED can change: never the
//! folders, the destination, encryption, or anything that removes data at once.

use crate::browse::{self, MapDir};
use crate::config::{self, Every, Plan, Run, Schedule};
use crate::core::{human_bytes as hb, Core};
use crate::places::tilde;
use keepr_engine::backup::Excluder;
use keepr_engine::repo::Repo;
use keepr_engine::retention::{self, Retention};
use keepr_engine::tree::NodeKind;
use keepr_engine::Id;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Folder names tools make and can make again, which a backup can leave out without loss.
pub const REBUILDABLE: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "target",
    ".gradle",
    "DerivedData",
    "Pods",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
    ".cache",
    "Caches",
    ".terraform",
    "bower_components",
];

const NOW_DAY: i64 = 86_400_000_000_000;

/// What one walk of a plan's latest snapshot found: the totals of folder names that repeat, and
/// facts about the folders the context lists.
#[derive(Default)]
struct Survey {
    /// By folder name: its size and how many times it appears, counting a folder of the same name
    /// inside another once (node_modules in node_modules).
    names: HashMap<String, (u64, u32)>,
    facts: HashMap<String, Facts>,
}

#[derive(Default, Clone)]
struct Facts {
    /// Bytes by file extension ("" for none).
    exts: HashMap<String, u64>,
    newest: i64,
    /// Bytes in files changed in the last 30 days.
    recent: u64,
    size: u64,
}

/// Surveys by snapshot and the folders asked about: a walk reads every listing in the snapshot.
type SurveyCache = HashMap<String, Arc<Survey>>;
static SURVEYS: Mutex<Option<SurveyCache>> = Mutex::new(None);

fn survey(repo: &Repo, tree: Id, listed: &HashSet<String>, key: String, read: &dyn Fn(u64)) -> Result<Arc<Survey>, String> {
    if let Some(s) = SURVEYS.lock().unwrap().get_or_insert_with(HashMap::new).get(&key) {
        return Ok(s.clone());
    }
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let mut sv = Survey::default();
    // (tree, its path, the listed folders it's in, the folder names above it)
    let mut todo: Vec<(Id, String, Vec<String>, Vec<String>)> = vec![(tree, String::new(), vec![], vec![])];
    let mut n = 0u64;
    while let Some((tid, path, inside, above)) = todo.pop() {
        crate::cancel::check()?;
        n += 1;
        if n.is_multiple_of(256) {
            read(n);
        }
        let t = repo.load_tree(&tid).map_err(|e| e.0)?;
        for n in &t.nodes {
            let p = if path.is_empty() { n.name.clone() } else { format!("{path}/{}", n.name) };
            match n.kind {
                NodeKind::Dir => {
                    // Sources (the top's folders) are named by their whole path, not grouped.
                    if !path.is_empty() && !above.contains(&n.name) {
                        let e = sv.names.entry(n.name.clone()).or_default();
                        e.0 += n.size;
                        e.1 += 1;
                    }
                    let mut inside = inside.clone();
                    if listed.contains(&p) {
                        inside.push(p.clone());
                        sv.facts.entry(p.clone()).or_default().size = n.size;
                    }
                    let mut above = above.clone();
                    above.push(n.name.clone());
                    if let Some(sub) = n.subtree {
                        todo.push((sub, p, inside, above));
                    }
                }
                NodeKind::File => {
                    if inside.is_empty() {
                        continue;
                    }
                    let ext = Path::new(&n.name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                    for f in &inside {
                        let fa = sv.facts.entry(f.clone()).or_default();
                        *fa.exts.entry(ext.clone()).or_default() += n.size;
                        fa.newest = fa.newest.max(n.mtime);
                        if now - n.mtime < 30 * NOW_DAY {
                            fa.recent += n.size;
                        }
                    }
                }
                NodeKind::Symlink => {}
            }
        }
    }
    let sv = Arc::new(sv);
    let mut c = SURVEYS.lock().unwrap();
    let c = c.get_or_insert_with(HashMap::new);
    if c.len() > 20 {
        c.clear();
    }
    c.insert(key, sv.clone());
    Ok(sv)
}

/// Whether the project a folder is in lists it in its .gitignore: None when it isn't in a git
/// project on this Mac (or isn't on this Mac any more). Read with the same rules Keepr's
/// backups use, without running git.
fn gitignored(path: &str) -> Option<bool> {
    let p = Path::new(path);
    if !p.is_dir() {
        return None;
    }
    let root = p.ancestors().find(|a| a.join(".git").exists())?;
    let mut b = ignore::gitignore::GitignoreBuilder::new(root);
    // Every .gitignore from the project's top down to the folder's own parent.
    let mut dirs: Vec<&Path> = p.ancestors().skip(1).take_while(|a| a.starts_with(root)).collect();
    dirs.reverse();
    for d in dirs {
        let g = d.join(".gitignore");
        if g.is_file() {
            b.add(g);
        }
    }
    let g = b.build().ok()?;
    Some(g.matched_path_or_any_parents(p, true).is_ignore())
}

fn ago(nanos: i64) -> String {
    if nanos <= 0 {
        return "unknown".into();
    }
    let days = (chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) - nanos) / NOW_DAY;
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        d if d < 60 => format!("{d} days ago"),
        d if d < 730 => format!("{} months ago", d / 30),
        d => format!("{} years ago", d / 365),
    }
}

fn signed(n: i64) -> String {
    if n < 0 {
        format!("−{}", hb(n.unsigned_abs()))
    } else {
        format!("+{}", hb(n as u64))
    }
}

fn every(s: &Schedule) -> String {
    const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    match s.every {
        Every::Minutes15 => "every 15 minutes".into(),
        Every::Hourly => "hourly".into(),
        Every::Daily => format!("daily at {}", s.at),
        // As next_run reads it: 7 is Sunday again.
        Every::Weekly => format!("weekly on {} at {}", DAYS[s.weekday as usize % 7], s.at),
        Every::Manual => "only when asked".into(),
    }
}

fn keeps(r: &Retention) -> String {
    let months = if r.monthly_months == 0 { "monthly forever".to_string() } else { format!("monthly for {} months", r.monthly_months) };
    format!(
        "every backup for {} hours, then daily for {} days, weekly for {} weeks, {months}; a deleted file's last version for {} days",
        r.all_hours, r.daily_days, r.weekly_weeks, r.keep_deleted_days
    )
}

/// Every folder on a size map, flattened.
fn flatten<'a>(d: &'a MapDir, out: &mut Vec<&'a MapDir>) {
    for k in &d.kids {
        out.push(k);
        flatten(k, out);
    }
}

/// Tells the Ask panel what the context is being made from, while it's made.
fn say(core: &Core, what: String) {
    (core.emit)("ask-progress", serde_json::Value::String(what));
}

/// "12,345".
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One plan's part of the context.
fn plan_text(core: &Core, p: &Plan, out: &mut String) -> Result<(), String> {
    let cfg = core.config.lock().unwrap().clone();
    let st = core.state.lock().unwrap().plans.get(&p.id).cloned().unwrap_or_default();
    let dest = cfg.destination(&p.destination).map(|d| d.name.clone()).unwrap_or_else(|| "(no destination)".into());
    let sources: Vec<String> = p
        .sources
        .iter()
        .map(|s| match s {
            config::Place::Folder { path, .. } => tilde(path),
            config::Place::Smb(m) => format!("a share on {}", m.server),
            config::Place::S3(_) => "an S3 bucket".into(),
        })
        .collect();
    let _ = writeln!(out, "\n## Plan \"{}\" (id {})", p.name, p.id);
    let _ = writeln!(
        out,
        "- {}, backs up {}{}",
        if p.enabled { "On" } else { "Off" },
        every(&p.schedule),
        if p.encrypted { ", encrypted" } else { "" }
    );
    let _ = writeln!(out, "- Backs up: {}", sources.join(", "));
    let _ = writeln!(out, "- To: {dest}");
    let _ = writeln!(
        out,
        "- Leaves out: {}; files a project's .gitignore lists: {}; cloud-only files: {}; what apps mark to skip: {}; files over a size: {}",
        if p.excludes.is_empty() { "nothing by rule".to_string() } else { p.excludes.join(", ") },
        if p.gitignore { "left out" } else { "backed up" },
        if p.skip_cloud_only { "left out" } else { "backed up" },
        if p.skip_marked { "left out" } else { "backed up" },
        if p.max_file_size > 0 { format!("over {} left out", hb(p.max_file_size)) } else { "no limit".into() },
    );
    let _ = writeln!(out, "- Keeps: {}", keeps(&p.retention));
    let _ = writeln!(
        out,
        "- {} snapshot{}{}; last backup {}",
        st.snapshots,
        if st.snapshots == 1 { "" } else { "s" },
        st.oldest.as_deref().map(|o| format!(" since {}", &o[..10.min(o.len())])).unwrap_or_default(),
        st.last_success.as_deref().unwrap_or("never")
    );
    let repo = core.repo(&p.id, false)?;
    let snaps = repo.snapshots().map_err(|e| e.0)?;
    let Some(latest) = snaps.last() else {
        let _ = writeln!(out, "- No backup yet.");
        return Ok(());
    };
    say(core, format!("Measuring the folders in {}…", p.name));
    let map = browse::size_map(core, &p.id, &latest.id.hex(), "")?;
    let total = map.size;
    let _ = writeln!(
        out,
        "- Latest backup: {} in {} files{}; stored in the destination: {} (every version, after de-duplication and compression)",
        hb(total),
        map.files,
        map.grew
            .map(|g| format!(" ({} since {})", signed(g), map.since.as_deref().map(|s| &s[..10.min(s.len())]).unwrap_or("?")))
            .unwrap_or_default(),
        hb(st.repo_bytes)
    );
    // The folders to describe: the biggest, and where it grew most.
    let mut all = vec![];
    flatten(&map, &mut all);
    let mut big: Vec<&MapDir> = all.iter().copied().filter(|d| d.size * 100 >= total.max(1) && d.depth <= 5).collect();
    big.sort_by_key(|d| std::cmp::Reverse(d.size));
    big.truncate(25);
    let mut grew: Vec<&MapDir> = all.iter().copied().filter(|d| d.grew.is_some_and(|g| g.unsigned_abs() >= 10_000_000)).collect();
    grew.sort_by_key(|d| std::cmp::Reverse(d.grew.unwrap_or(0).unsigned_abs()));
    for d in grew.into_iter().take(10) {
        if !big.iter().any(|b| b.path == d.path) {
            big.push(d);
        }
    }
    // A chain of folders each nearly all of the one above (app › node_modules › react) is one
    // folder: the one tools rebuild if it's among them, else the top of the chain.
    let parent_of = |p: &str| p.rsplit_once('/').map(|(a, _)| a.to_string());
    let mut drop: HashSet<String> = HashSet::new();
    for d in &big {
        let Some(up) = parent_of(&d.path) else { continue };
        let Some(u) = big.iter().find(|b| b.path == up) else { continue };
        if d.size * 10 >= u.size * 9 {
            if REBUILDABLE.contains(&d.name.as_str()) {
                drop.insert(u.path.clone());
            } else {
                drop.insert(d.path.clone());
            }
        }
    }
    big.retain(|d| !drop.contains(&d.path));
    let listed: HashSet<String> = big.iter().map(|d| d.path.clone()).collect();
    let mut key: Vec<&String> = listed.iter().collect();
    key.sort();
    let sv = survey(
        &repo,
        latest.tree,
        &listed,
        format!("{}:{}", latest.id.hex(), key.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("|")),
        &|n| say(core, format!("Reading {}'s latest backup: {} folders…", p.name, thousands(n))),
    )?;
    say(core, format!("Working out the space {}'s versions take…", p.name));
    // Worked out once: if it can't be, the stored figures are left out rather than each folder's
    // asking again, which would read every listing again.
    let stored_ok = match crate::stored::warm(core, &p.id, &|n| {
        say(core, format!("Working out the space {}'s versions take: {} listings read…", p.name, thousands(n)))
    }) {
        Ok(()) => true,
        Err(_) => {
            crate::cancel::check()?;
            false
        }
    };
    let _ = writeln!(out, "\nBiggest folders in the latest backup (size · change since the comparison · stored · stored this week · file types by size · newest file · changed in 30 days · .gitignore):");
    for d in &big {
        let f = sv.facts.get(&d.path).cloned().unwrap_or_default();
        let mut exts: Vec<(&String, &u64)> = f.exts.iter().collect();
        exts.sort_by_key(|(_, b)| std::cmp::Reverse(**b));
        let types = exts
            .iter()
            .take(3)
            .map(|(e, b)| {
                format!("{}% {}", (**b * 100) / f.size.max(1), if e.is_empty() { "no extension".to_string() } else { format!(".{e}") })
            })
            .collect::<Vec<_>>()
            .join(", ");
        let stored = if stored_ok { crate::stored::stored_of(core, &p.id, &d.path) } else { None };
        let git = match gitignored(&d.path) {
            Some(true) => "listed in its project's .gitignore",
            Some(false) => "in a git project, not ignored",
            None => "not in a git project",
        };
        let _ = writeln!(
            out,
            "- {} · {} · {} · {} · {} · {} · {} · {} · {}",
            tilde(&d.path),
            hb(d.size),
            d.grew.map(signed).unwrap_or_else(|| "—".into()),
            stored.map(|s| hb(s.0)).unwrap_or_else(|| "—".into()),
            stored.map(|s| hb(s.1)).unwrap_or_else(|| "—".into()),
            if types.is_empty() { "—".into() } else { types },
            ago(f.newest),
            hb(f.recent),
            git
        );
    }
    let mut names: Vec<(&String, &(u64, u32))> =
        sv.names.iter().filter(|(_, (b, n))| *n >= 2 && *b >= (total / 50).min(50_000_000)).collect();
    names.sort_by_key(|(_, (b, _))| std::cmp::Reverse(*b));
    if !names.is_empty() {
        let _ = writeln!(out, "\nFolder names that repeat in the latest backup (total of the outermost of each):");
        for (n, (b, c)) in names.into_iter().take(12) {
            let _ = writeln!(
                out,
                "- {n}: {} in {c} places{}",
                hb(*b),
                if REBUILDABLE.contains(&n.as_str()) { " (tools rebuild these)" } else { "" }
            );
        }
    }
    // Folders whose stored versions take space but aren't in the latest backup.
    if let Some(Ok(sm)) = stored_ok.then(|| crate::stored::stored_map(core, &p.id, "")) {
        let mut sall = vec![];
        flatten(&sm, &mut sall);
        let mut gone: Vec<&MapDir> = sall
            .into_iter()
            .filter(|d| d.special.is_none() && d.depth <= 4 && d.size * 50 >= sm.size.max(1) && !all.iter().any(|a| a.path == d.path))
            .collect();
        gone.sort_by_key(|d| std::cmp::Reverse(d.size));
        if !gone.is_empty() {
            let _ = writeln!(out, "\nStored but not in the latest backup (old versions of folders deleted or left out since):");
            for d in gone.into_iter().take(8) {
                let _ = writeln!(out, "- {} · {} stored", tilde(&d.path), hb(d.size));
            }
        }
        for k in sm.kids.iter().filter(|k| k.special.is_some()) {
            let _ = writeln!(out, "- {}: {}", k.name, hb(k.size));
        }
    }
    Ok(())
}

/// The text a question carries, for every plan Ask may see. `plan`, when given, comes first.
pub fn context(core: &Core, plan: Option<&str>) -> Result<String, String> {
    let cfg = core.config.lock().unwrap().clone();
    if !cfg.settings.ask {
        return Err("Ask is off in Settings.".into());
    }
    let st = core.state.lock().unwrap().clone();
    let mut out = String::new();
    let total: u64 = cfg.plans.iter().filter_map(|p| st.plans.get(&p.id)).map(|s| s.repo_bytes).sum();
    let _ = writeln!(out, "What Keepr knows, as of {} (sizes in binary units):", chrono::Local::now().format("%a %e %b %Y, %H:%M"));
    let _ = writeln!(out, "- Space used in all destinations: {}", hb(total));
    let mut plans: Vec<&Plan> = cfg.plans.iter().collect();
    if let Some(id) = plan {
        plans.sort_by_key(|p| p.id != id);
    }
    let left_out: Vec<&str> = plans.iter().filter(|p| p.ask_exclude).map(|p| p.name.as_str()).collect();
    if !left_out.is_empty() {
        let _ = writeln!(out, "- Left out of Ask by the user, so not described: {}", left_out.join(", "));
    }
    for p in plans.iter().filter(|p| !p.ask_exclude) {
        crate::cancel::check()?;
        if let Err(e) = plan_text(core, p, &mut out) {
            crate::cancel::check()?;
            let _ = writeln!(out, "- Couldn't read this plan's backup: {e}");
        }
    }
    let seen: HashSet<&str> = plans.iter().filter(|p| !p.ask_exclude).map(|p| p.id.as_str()).collect();
    let runs: Vec<&Run> = st.history.iter().rev().filter(|r| seen.contains(r.plan.as_str())).take(15).collect();
    if !runs.is_empty() {
        let _ = writeln!(out, "\n## Recent runs, newest first");
        for r in runs {
            let name = cfg.plans.iter().find(|p| p.id == r.plan).map(|p| p.name.as_str()).unwrap_or("?");
            let _ = writeln!(
                out,
                "- {} · {name} · {} · {}{}{}",
                &r.started[..16.min(r.started.len())],
                r.kind,
                r.result,
                if r.stored_bytes > 0 { format!(" · {} stored", hb(r.stored_bytes)) } else { String::new() },
                if r.message.is_empty() { String::new() } else { format!(" · {}", pathless(&r.message)) }
            );
        }
    }
    Ok(out)
}

/// A run's message with the paths taken out: messages name files ("1 file couldn't be read
/// (first: ~/Documents/Budget.numbers)", "~/Old removed from 3 snapshots"), and Ask never hears a
/// file's name. A path runs from a "/" or "~/" that starts a word to the ")" that closes it, the
/// " removed from" after it, or the end of its part of the message (the parts are joined by " · "),
/// since a path can have spaces in it.
fn pathless(msg: &str) -> String {
    msg.split(" · ")
        .map(|part| {
            let mut out = String::new();
            let mut rest = part;
            loop {
                let b = rest.as_bytes();
                let start =
                    (0..b.len()).find(|&i| (i == 0 || matches!(b[i - 1], b' ' | b'(')) && (b[i] == b'/' || b[i..].starts_with(b"~/")));
                let Some(i) = start else {
                    out.push_str(rest);
                    break;
                };
                let path = &rest[i..];
                let end = [")", " removed from"].iter().filter_map(|e| path.find(e)).min().unwrap_or(path.len());
                out.push_str(&rest[..i]);
                out.push_str(if out.is_empty() { "Something" } else { "a file" });
                rest = &path[end..];
            }
            out
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ---- proposals ----

/// What an answer may change, as the keys of its keepr-change block.
pub const ALLOWED: &[&str] =
    &["plan", "why", "addExcludes", "removeExcludes", "gitignore", "skipCloudOnly", "skipMarked", "maxFileSize", "retention", "schedule"];

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Change {
    plan: String,
    #[serde(default)]
    why: String,
    #[serde(default)]
    add_excludes: Vec<String>,
    #[serde(default)]
    remove_excludes: Vec<String>,
    gitignore: Option<bool>,
    skip_cloud_only: Option<bool>,
    skip_marked: Option<bool>,
    max_file_size: Option<u64>,
    retention: Option<serde_json::Value>,
    schedule: Option<serde_json::Value>,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    what: String,
    before: String,
    after: String,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LeftOut {
    files: u64,
    bytes: u64,
    /// Of those, changed in the last 7 days.
    recent_files: u64,
    /// The biggest of what's left out, by path as the user knows it.
    folders: Vec<(String, u64)>,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    plan_id: String,
    plan_name: String,
    why: String,
    lines: Vec<Line>,
    left_out: Option<LeftOut>,
    /// Snapshots kept now, and after the change.
    snapshots: Option<(u32, u32, u32)>,
    warnings: Vec<String>,
}

/// Overlays the fields given on what's there, so a change can name only what it changes.
fn overlay<T: Serialize + for<'de> Deserialize<'de>>(now: &T, with: &serde_json::Value, what: &str) -> Result<T, String> {
    let mut v = serde_json::to_value(now).map_err(|e| e.to_string())?;
    let (Some(m), Some(w)) = (v.as_object_mut(), with.as_object()) else {
        return Err(format!("The {what} isn't written as Keepr expects."));
    };
    for (k, x) in w {
        if !m.contains_key(k) {
            return Err(format!("The {what} has no setting called {k}."));
        }
        m.insert(k.clone(), x.clone());
    }
    serde_json::from_value(v).map_err(|e| format!("The {what} isn't one Keepr can use: {e}"))
}

/// The plan as the change would leave it.
fn changed_plan(core: &Core, raw: &serde_json::Value) -> Result<(Plan, Plan, Change), String> {
    // The window hides Ask when it's off; this holds even if a page still showing it asks.
    if !core.config.lock().unwrap().settings.ask {
        return Err("Ask is off in Settings.".into());
    }
    let obj = raw.as_object().ok_or("The change isn't written as Keepr expects.")?;
    let refused: Vec<&String> = obj.keys().filter(|k| !ALLOWED.contains(&k.as_str())).collect();
    if !refused.is_empty() {
        return Err(format!(
            "Ask can't change {}: only what a plan leaves out, its schedule and how long it keeps versions. Change the rest in the plan yourself.",
            refused.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    let c: Change = serde_json::from_value(raw.clone()).map_err(|e| format!("The change isn't one Keepr can use: {e}"))?;
    let before = {
        let cfg = core.config.lock().unwrap();
        cfg.plans
            .iter()
            .find(|p| p.id == c.plan || p.name == c.plan)
            .cloned()
            .ok_or_else(|| format!("There's no plan called {}.", c.plan))?
    };
    if before.ask_exclude {
        return Err(format!("{} is left out of Ask.", before.name));
    }
    let mut p = before.clone();
    for x in c.add_excludes.iter().map(|x| x.trim()).filter(|x| !x.is_empty()) {
        if !p.excludes.iter().any(|e| e == x) {
            p.excludes.push(x.to_string());
        }
    }
    p.excludes.retain(|e| !c.remove_excludes.iter().any(|r| r.trim() == e));
    Excluder::new(&p.excludes).map_err(|e| e.0)?;
    if let Some(g) = c.gitignore {
        p.gitignore = g;
    }
    if let Some(g) = c.skip_cloud_only {
        p.skip_cloud_only = g;
    }
    if let Some(g) = c.skip_marked {
        p.skip_marked = g;
    }
    if let Some(m) = c.max_file_size {
        p.max_file_size = m;
    }
    if let Some(r) = &c.retention {
        p.retention = overlay(&p.retention, r, "retention")?;
    }
    if let Some(s) = &c.schedule {
        valid_schedule(s)?;
        p.schedule = overlay(&p.schedule, s, "schedule")?;
    }
    Ok((before, p, c))
}

/// A proposed schedule's fields, each as the plan editor could set it: `every` one of its
/// choices, `at` a time of day as HH:MM, `weekday` 0 (Sunday) to 6 (Saturday).
fn valid_schedule(s: &serde_json::Value) -> Result<(), String> {
    if let Some(e) = s.get("every") {
        const EVERY: &[&str] = &["minutes15", "hourly", "daily", "weekly", "manual"];
        if !e.as_str().is_some_and(|e| EVERY.contains(&e)) {
            return Err(format!("The schedule can't be every {e}: it's one of {}.", EVERY.join(", ")));
        }
    }
    if let Some(at) = s.get("at") {
        let ok = at.as_str().is_some_and(|t| {
            let b = t.as_bytes();
            let two = |x: &[u8]| x.iter().all(u8::is_ascii_digit).then(|| (x[0] - b'0') * 10 + (x[1] - b'0'));
            b.len() == 5 && b[2] == b':' && two(&b[..2]).is_some_and(|h| h < 24) && two(&b[3..]).is_some_and(|m| m < 60)
        });
        if !ok {
            return Err(format!("The schedule's time {at} isn't a time of day written as HH:MM (00:00 to 23:59)."));
        }
    }
    if let Some(w) = s.get("weekday") {
        if !w.as_u64().is_some_and(|w| w <= 6) {
            return Err(format!("The schedule's weekday {w} isn't one: it's 0 for Sunday to 6 for Saturday."));
        }
    }
    Ok(())
}

fn onoff(b: bool) -> String {
    (if b { "on" } else { "off" }).into()
}

fn size_limit(b: u64) -> String {
    if b == 0 {
        "no limit".into()
    } else {
        hb(b)
    }
}

/// One line per setting that changes, each saying only that setting's before and after; an
/// empty before is a rule added, an empty after one removed.
fn diff(a: &Plan, b: &Plan) -> Vec<Line> {
    let mut out = vec![];
    let mut line = |what: &str, x: String, y: String| {
        if x != y {
            out.push(Line { what: what.into(), before: x, after: y });
        }
    };
    let added: Vec<&str> = b.excludes.iter().filter(|e| !a.excludes.contains(e)).map(|s| s.as_str()).collect();
    let removed: Vec<&str> = a.excludes.iter().filter(|e| !b.excludes.contains(e)).map(|s| s.as_str()).collect();
    line("Also leave out", String::new(), added.join(", "));
    line("Stop leaving out", removed.join(", "), String::new());
    line("Leave out what .gitignore lists", onoff(a.gitignore), onoff(b.gitignore));
    line("Leave out cloud-only files", onoff(a.skip_cloud_only), onoff(b.skip_cloud_only));
    line("Leave out what apps mark", onoff(a.skip_marked), onoff(b.skip_marked));
    line("Leave out files over", size_limit(a.max_file_size), size_limit(b.max_file_size));
    let n = |v: u32, unit: &str| format!("{v} {unit}{}", if v == 1 { "" } else { "s" });
    let (x, y) = (&a.retention, &b.retention);
    line("Keep every backup for", n(x.all_hours, "hour"), n(y.all_hours, "hour"));
    line("Then one a day for", n(x.daily_days, "day"), n(y.daily_days, "day"));
    line("Then one a week for", n(x.weekly_weeks, "week"), n(y.weekly_weeks, "week"));
    let months = |m: u32| if m == 0 { "ever".to_string() } else { n(m, "month") };
    line("Then one a month for", months(x.monthly_months), months(y.monthly_months));
    line("A deleted file's last version for", n(x.keep_deleted_days, "day"), n(y.keep_deleted_days, "day"));
    line("Back up", every(&a.schedule), every(&b.schedule));
    out
}

/// What the latest backup has that the new rules leave out.
fn left_out(repo: &Repo, a: &Plan, b: &Plan) -> Result<Option<LeftOut>, String> {
    let more_rules = b.excludes.iter().any(|e| !a.excludes.contains(e));
    let smaller = b.max_file_size > 0 && (a.max_file_size == 0 || b.max_file_size < a.max_file_size);
    if !more_rules && !smaller {
        return Ok(None);
    }
    let Some(snap) = repo.snapshots().map_err(|e| e.0)?.pop() else { return Ok(None) };
    let (old, new) = (Excluder::new(&a.excludes).map_err(|e| e.0)?, Excluder::new(&b.excludes).map_err(|e| e.0)?);
    let week = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) - 7 * NOW_DAY;
    let mut lo = LeftOut::default();
    let mut folders: Vec<(String, u64)> = vec![];
    // Counts a subtree left out whole: its files changed this week.
    fn recent(repo: &Repo, t: Id, week: i64) -> Result<u64, String> {
        let mut n = 0;
        for x in &repo.load_tree(&t).map_err(|e| e.0)?.nodes {
            match (x.kind, x.subtree) {
                (NodeKind::Dir, Some(s)) => n += recent(repo, s, week)?,
                (NodeKind::File, _) if x.mtime >= week => n += 1,
                _ => {}
            }
        }
        Ok(n)
    }
    let mut todo: Vec<(Id, PathBuf)> = vec![(snap.tree, PathBuf::new())];
    while let Some((tid, path)) = todo.pop() {
        crate::cancel::check()?;
        for n in &repo.load_tree(&tid).map_err(|e| e.0)?.nodes {
            let p = if path.as_os_str().is_empty() { PathBuf::from(&n.name) } else { path.join(&n.name) };
            let dir = n.kind == NodeKind::Dir;
            let top = path.as_os_str().is_empty();
            if !top && new.excluded(&p, &n.name, dir) && !old.excluded(&p, &n.name, dir) {
                lo.bytes += n.size;
                lo.files += if dir { n.files } else { 1 };
                lo.recent_files += match (dir, n.subtree) {
                    (true, Some(s)) => recent(repo, s, week)?,
                    (false, _) => (n.mtime >= week) as u64,
                    _ => 0,
                };
                if dir {
                    folders.push((tilde(&p.to_string_lossy()), n.size));
                }
                continue;
            }
            if dir {
                if let Some(s) = n.subtree {
                    todo.push((s, p));
                }
            } else if smaller && n.size > b.max_file_size && (a.max_file_size == 0 || n.size <= a.max_file_size) {
                lo.bytes += n.size;
                lo.files += 1;
                lo.recent_files += (n.mtime >= week) as u64;
            }
        }
    }
    folders.sort_by_key(|f| std::cmp::Reverse(f.1));
    folders.truncate(8);
    lo.folders = folders;
    Ok(Some(lo))
}

/// What a proposed change would do, and what to warn about, without changing anything.
pub fn check(core: &Core, raw: &serde_json::Value) -> Result<Checked, String> {
    let (a, b, c) = changed_plan(core, raw)?;
    let lines = diff(&a, &b);
    if lines.is_empty() {
        return Err(format!("{} already has these settings.", a.name));
    }
    let mut warnings = vec![];
    let repo = core.repo(&a.id, false)?;
    let lo = left_out(&repo, &a, &b)?;
    let total = core.state.lock().unwrap().plans.get(&a.id).map_or(0, |s| s.versions_bytes);
    let latest = repo.snapshots().map_err(|e| e.0)?.last().map_or(0, |s| s.stats.bytes);
    if let Some(l) = &lo {
        if l.recent_files > 0 {
            warnings.push(format!("{} of the files it leaves out changed this week.", l.recent_files));
        }
        let unknown: Vec<&String> =
            l.folders.iter().map(|(p, _)| p).filter(|p| !REBUILDABLE.iter().any(|r| p.rsplit('/').next() == Some(*r))).take(3).collect();
        if !unknown.is_empty() {
            warnings.push(format!(
                "It leaves out folders that tools don't rebuild: {}. Be sure you won't need them back.",
                unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        if latest > 0 && l.bytes * 4 > latest {
            warnings.push(format!("It leaves out {}% of what the plan backs up.", l.bytes * 100 / latest));
        }
    }
    if !b.gitignore && a.gitignore {
        warnings.push(
            "Files your projects' .gitignore files list will be backed up again, which can be a lot (node_modules and the like).".into(),
        );
    }
    if b.gitignore && !a.gitignore {
        warnings.push("Files your projects' .gitignore files list will be left out. Keepr can't count them in advance.".into());
    }
    if b.schedule.every == Every::Manual && a.schedule.every != Every::Manual {
        warnings.push(format!("{} won't back up on its own any more: only when you ask it to.", a.name));
    }
    if !b.skip_cloud_only && a.skip_cloud_only {
        warnings.push("Cloud-only files will be downloaded to back them up, which takes space on this Mac and time.".into());
    }
    let mut snapshots = None;
    if a.retention != b.retention {
        let snaps = repo.snapshots().map_err(|e| e.0)?;
        let times: Vec<chrono::DateTime<chrono::Local>> = snaps.iter().filter_map(|s| retention::parse_time(&s.time)).collect();
        let now = chrono::Local::now();
        let (x, y) = (retention::keep(&a.retention, &times, now).len() as u32, retention::keep(&b.retention, &times, now).len() as u32);
        // There can be more snapshots than the current settings keep: the tidy-up runs at most
        // once a day. What this change adds to it is what it should be judged on.
        let there = snaps.len() as u32;
        if y < x {
            let gone = there.saturating_sub(y);
            let ours = if gone > x - y { format!(", {} of them because of this change", x - y) } else { String::new() };
            warnings.push(format!(
                "The next tidy-up removes {gone} snapshots{ours}, and the versions only they hold can't be restored afterwards."
            ));
        }
        snapshots = Some((there, x, y));
    }
    let _ = total;
    Ok(Checked { plan_id: a.id.clone(), plan_name: a.name.clone(), why: c.why, lines, left_out: lo, snapshots, warnings })
}

fn changes_dir(core: &Core) -> PathBuf {
    core.dir.join("ask-changes")
}

/// What a change made with Ask touched, kept so that undoing it puts back only that: the plan's
/// other settings, and later changes to it, stay as they are.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Undo {
    plan: String,
    /// Rules it added, and rules it took away.
    #[serde(default)]
    added: Vec<String>,
    #[serde(default)]
    removed: Vec<String>,
    /// By setting ("gitignore", "retention.dailyDays", "schedule.at"): its value before and after.
    fields: BTreeMap<String, (serde_json::Value, serde_json::Value)>,
}

/// The settings Ask may change, other than the rules, one by one: retention and schedule by
/// their fields, so two changes to different ones don't stand in each other's way.
fn settings(p: &Plan) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let v = serde_json::to_value(p).map_err(|e| e.to_string())?;
    let mut out = BTreeMap::new();
    for k in ["gitignore", "skipCloudOnly", "skipMarked", "maxFileSize"] {
        out.insert(k.to_string(), v[k].clone());
    }
    for k in ["retention", "schedule"] {
        for (f, x) in v[k].as_object().into_iter().flatten() {
            out.insert(format!("{k}.{f}"), x.clone());
        }
    }
    Ok(out)
}

/// Saves the change through the plan editor's path, and records it in Activity with what it
/// changed, so it can be undone. Returns the run's id.
pub fn apply(core: &Core, raw: &serde_json::Value) -> Result<String, String> {
    let checked = check(core, raw)?;
    let (before, after, c) = changed_plan(core, raw)?;
    let (was, now) = (settings(&before)?, settings(&after)?);
    let undo = Undo {
        plan: before.id.clone(),
        added: after.excludes.iter().filter(|e| !before.excludes.contains(e)).cloned().collect(),
        removed: before.excludes.iter().filter(|e| !after.excludes.contains(e)).cloned().collect(),
        fields: was.into_iter().filter_map(|(k, x)| (now.get(&k) != Some(&x)).then(|| (k.clone(), (x, now[&k].clone())))).collect(),
    };
    crate::save_plan_now(core, after, None)?;
    let id = config::new_id();
    let dir = changes_dir(core);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec_pretty(&undo).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut msg = format!("Changed with Ask: {}", checked.lines.iter().map(|l| l.what.as_str()).collect::<Vec<_>>().join(", "));
    if !c.why.is_empty() {
        msg = format!("{msg}. {}", c.why);
    }
    record(
        core,
        &id,
        &before.id,
        msg,
        checked
            .lines
            .iter()
            .map(|l| match (l.before.is_empty(), l.after.is_empty()) {
                (true, _) => format!("{}: {}", l.what, l.after),
                (_, true) => format!("{}: {}", l.what, l.before),
                _ => format!("{}: {} → {}", l.what, l.before, l.after),
            })
            .collect(),
    );
    Ok(id)
}

fn record(core: &Core, id: &str, plan: &str, message: String, log: Vec<String>) {
    let now = chrono::Utc::now().to_rfc3339();
    let run = Run {
        id: id.into(),
        plan: plan.into(),
        kind: "change".into(),
        started: now.clone(),
        finished: now,
        result: "ok".into(),
        message,
        files: 0,
        changed: 0,
        read_bytes: 0,
        added_bytes: 0,
        stored_bytes: 0,
        dup_bytes: 0,
        log: vec![],
        log_skipped: 0,
    };
    if !core.frozen && !log.is_empty() {
        let logs = core.dir.join("logs");
        let _ = std::fs::create_dir_all(&logs);
        let _ = std::fs::write(logs.join(format!("{id}.log")), log.join("\n"));
    }
    core.state.lock().unwrap().record(run);
    core.save_state();
    core.changed();
}

/// The changes made with Ask that can still be undone, by run id.
pub fn undoable(core: &Core) -> Vec<String> {
    std::fs::read_dir(changes_dir(core))
        .map(|d| d.filter_map(|e| e.ok()?.path().file_stem().map(|s| s.to_string_lossy().into_owned())).collect())
        .unwrap_or_default()
}

/// Puts back what a change made with Ask changed, and only that: the rules it added go and those
/// it took away come back, and each setting it changed returns to what it was, as long as it is
/// still what the change left it (else the user has changed it since, and that stands).
pub fn undo(core: &Core, id: &str) -> Result<(), String> {
    let f = changes_dir(core).join(format!("{id}.json"));
    let raw = std::fs::read(&f).map_err(|_| "That change can't be undone any more.")?;
    // Keepr 1.0.19 and before kept the whole plan, which would undo every edit made since too.
    let Ok(u) = serde_json::from_slice::<Undo>(&raw) else {
        let _ = std::fs::remove_file(&f);
        return Err(
            "That change was made by an earlier Keepr, which can't undo it without undoing later edits too. Change it in the plan instead."
                .into(),
        );
    };
    let Some(mut plan) = core.config.lock().unwrap().plans.iter().find(|p| p.id == u.plan).cloned() else {
        let _ = std::fs::remove_file(&f);
        return Err("That plan has been removed.".into());
    };
    let now = settings(&plan)?;
    if u.fields.iter().any(|(k, (_, after))| now.get(k) != Some(after)) {
        return Err(format!("{}'s settings have changed since; change them in the plan instead.", plan.name));
    }
    let mut v = serde_json::to_value(&plan).map_err(|e| e.to_string())?;
    for (k, (before, _)) in &u.fields {
        match k.split_once('.') {
            Some((group, field)) => v[group][field] = before.clone(),
            None => v[k.as_str()] = before.clone(),
        }
    }
    let excludes = plan.excludes;
    plan = serde_json::from_value(v).map_err(|e| e.to_string())?;
    plan.excludes = excludes.into_iter().filter(|e| !u.added.contains(e)).collect();
    for r in &u.removed {
        if !plan.excludes.contains(r) {
            plan.excludes.push(r.clone());
        }
    }
    let plan_id = plan.id.clone();
    crate::save_plan_now(core, plan, None)?;
    let _ = std::fs::remove_file(&f);
    record(core, &config::new_id(), &plan_id, "Undid a change made with Ask".into(), vec![]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_names_only_what_it_may() {
        let raw = serde_json::json!({"plan": "x", "destination": "d2", "encrypted": false});
        let obj = raw.as_object().unwrap();
        let refused: Vec<&String> = obj.keys().filter(|k| !ALLOWED.contains(&k.as_str())).collect();
        assert_eq!(refused.len(), 2);
    }

    #[test]
    fn overlay_keeps_what_isnt_named() {
        let r = Retention { all_hours: 24, daily_days: 30, weekly_weeks: 8, monthly_months: 0, keep_deleted_days: 90 };
        let r2: Retention = overlay(&r, &serde_json::json!({"dailyDays": 14}), "retention").unwrap();
        assert_eq!((r2.all_hours, r2.daily_days, r2.monthly_months), (24, 14, 0));
        assert!(overlay::<Retention>(&r, &serde_json::json!({"forever": 1}), "retention").is_err());
    }
}
