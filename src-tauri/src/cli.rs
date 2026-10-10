// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! `keepr <command>`: the app's binary without a window, for scripts, cron and a terminal.
//!
//! Results go to stdout, as text for people or as JSON with `--json`; progress, warnings and
//! errors go to stderr, so a pipe gets only the result. Exit status: 0 done, 1 failed, 2 the
//! command line was wrong, 3 a restore didn't match its sources, 130 stopped with Ctrl-C.
//! Commands that change a backup or the history refuse while the app is open (see `locked_core`);
//! the ones that only read run beside it.

use crate::config::{self, Place, Run};
use crate::core::{human_bytes, Core, Job};
use crate::{keychain, overview_of, places, system, verify, Core_};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use keepr_engine::restore::{Conflict, Target};
use serde::Serialize;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const AFTER_HELP: &str = "\
A plan or destination is named by its id or its name (any case); `keepr plans` and
`keepr destinations` list them. Commands that change a backup refuse while the Keepr app
is open, so quit it first; the others run beside it.

Exit status: 0 done, 1 failed, 2 wrong command line, 3 a restore didn't match, 130 Ctrl-C.";

#[derive(Parser)]
#[command(name = "keepr", version, about = "Keepr's backups from the command line.", after_help = AFTER_HELP)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct Out {
    /// Print the result as JSON, for scripts.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct Progress {
    /// Don't show progress on stderr.
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Command {
    /// List the plans, with their ids.
    Plans(Out),
    /// List the destinations, with their ids.
    Destinations(Out),
    /// How each plan is doing. Exits 1 if any needs attention (failed, waiting, stale or never backed up).
    Status {
        /// Only these plans, by id or name (all if none).
        plans: Vec<String>,
        #[command(flatten)]
        out: Out,
    },
    /// List a plan's snapshots, oldest first.
    Snapshots {
        /// The plan, by id or name.
        plan: String,
        #[command(flatten)]
        out: Out,
    },
    /// Back up plans now, whatever their schedule and wait conditions.
    #[command(name = "back-up")]
    BackUp {
        /// The plans to back up.
        #[arg(required_unless_present = "all")]
        plans: Vec<String>,
        /// Back up every plan that's turned on.
        #[arg(long, conflicts_with = "plans")]
        all: bool,
        /// Read every file again (a full backup), not only what changed.
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        out: Out,
        #[command(flatten)]
        progress: Progress,
    },
    /// Check a plan's stored data: a sample of it, or all of it with --all-data.
    Check {
        /// The plan, by id or name.
        plan: String,
        /// Read every pack, not a 5% sample.
        #[arg(long)]
        all_data: bool,
        #[command(flatten)]
        out: Out,
        #[command(flatten)]
        progress: Progress,
    },
    /// Apply a plan's version rules now, and free the space of what they let go.
    Tidy {
        /// The plan, by id or name.
        plan: String,
        #[command(flatten)]
        out: Out,
        #[command(flatten)]
        progress: Progress,
    },
    /// Restore files or folders from a snapshot.
    Restore {
        /// The plan, by id or name.
        plan: String,
        /// Original full paths of what to restore (everything in the snapshot if none).
        paths: Vec<String>,
        /// Restore into this folder; items keep their folders inside it.
        #[arg(long, value_name = "FOLDER", required_unless_present = "original", conflicts_with = "original")]
        to: Option<PathBuf>,
        /// Restore to where the items were.
        #[arg(long)]
        original: bool,
        /// The snapshot, by id or the start of one (the newest if not given).
        #[arg(long, value_name = "ID")]
        snapshot: Option<String>,
        /// What to do when a file is already there.
        #[arg(long, value_enum, default_value_t = OnConflict::KeepBoth)]
        conflict: OnConflict,
        #[command(flatten)]
        out: Out,
        #[command(flatten)]
        progress: Progress,
    },
    /// Restore a plan's newest snapshot into a folder and compare it byte for byte with the sources. Exits 3 on a mismatch.
    #[command(name = "verify-restore")]
    VerifyRestore {
        /// The plan, by id or name.
        plan: String,
        /// An empty folder to restore into.
        folder: PathBuf,
    },
    /// Take a file or folder, by its original full path, out of every snapshot of a plan and free the space only it used.
    #[command(name = "remove-path")]
    RemovePath {
        /// The plan, by id or name.
        plan: String,
        /// The file or folder's original full path.
        path: String,
        /// Confirm: this can't be undone.
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        out: Out,
        #[command(flatten)]
        progress: Progress,
    },
    /// Rename a plan's backup folder to match the plan's name.
    #[command(name = "rename-plan-folder")]
    RenamePlanFolder {
        /// The plan, by id or name.
        plan: String,
    },
    /// List every object in an S3, B2 or R2 destination's bucket.
    #[command(name = "list-objects")]
    ListObjects {
        /// The destination, by id or name.
        destination: String,
        #[command(flatten)]
        out: Out,
    },
    /// Copy one plan's backup password in the Keychain to another plan.
    #[command(name = "copy-password")]
    CopyPassword {
        /// The plan whose password to copy, by id or name.
        from: String,
        /// The plan to give it to, by id or name.
        to: String,
    },
    /// Print a completion script for a shell.
    Completions {
        /// The shell to write it for.
        shell: clap_complete::Shell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum OnConflict {
    /// Keep the file that's there and restore beside it as "name (restored)".
    KeepBoth,
    /// Restore over the file that's there.
    Replace,
    /// Leave the file that's there and don't restore that one.
    Skip,
}

/// The flags of earlier versions, still taken, each as its command now.
const OLD_FLAGS: &[(&str, &str)] = &[
    ("--back-up", "back-up"),
    ("--verify-restore", "verify-restore"),
    ("--rename-plan-folder", "rename-plan-folder"),
    ("--list-destination", "list-objects"),
    ("--copy-plan-password", "copy-password"),
    ("--remove-path", "remove-path"),
];

/// Whether these arguments are for the command line rather than the app. Finder and Login Items
/// start the app with none (an old macOS adds a `-psn_` one), and `tauri dev` with none either.
pub fn wanted(args: &[String]) -> bool {
    args.first().is_some_and(|a| !a.starts_with("-psn_"))
}

/// Runs the command line and returns the exit status.
pub fn main(args: &[String]) -> i32 {
    // A closed pipe (`keepr plans | head`) ends the process quietly, as for other tools, rather
    // than as a failed write.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    // Before anything reads a file: both the app and these commands may meet cloud-only files.
    system::allow_cloud_downloads();
    let mut argv = vec!["keepr".to_string()];
    let mut rest = args.to_vec();
    if let Some((old, new)) = OLD_FLAGS.iter().find(|(old, _)| rest.first().map(String::as_str) == Some(*old)) {
        eprintln!("keepr: {old} is now `keepr {new}`; the old form will go in a later version");
        rest[0] = new.to_string();
        // The old form didn't ask.
        if *new == "remove-path" {
            rest.push("--yes".into());
        }
    }
    argv.extend(rest);
    let cli = match Cli::try_parse_from(&argv) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return e.exit_code();
        }
    };
    match run(cli.command) {
        Ok(code) => code,
        Err(Failure::Usage(e)) => {
            eprintln!("keepr: {e}");
            2
        }
        Err(Failure::Failed(e)) => {
            eprintln!("keepr: {e}");
            1
        }
    }
}

enum Failure {
    /// The command line asked for something that isn't there, or for something it must confirm.
    Usage(String),
    Failed(String),
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Failed(e)
    }
}

impl From<&str> for Failure {
    fn from(e: &str) -> Self {
        Failure::Failed(e.to_string())
    }
}

type Res<T> = Result<T, Failure>;

fn run(cmd: Command) -> Res<i32> {
    match cmd {
        Command::Plans(out) => plans(out),
        Command::Destinations(out) => destinations(out),
        Command::Status { plans, out } => status(&plans, out),
        Command::Snapshots { plan, out } => snapshots(&plan, out),
        Command::BackUp { plans, all, full, out, progress } => {
            let core = locked_core(&progress)?;
            let ids = if all {
                core.config.lock().unwrap().plans.iter().filter(|p| p.enabled).map(|p| p.id.clone()).collect()
            } else {
                plans.iter().map(|p| plan_id(&core, p)).collect::<Res<Vec<_>>>()?
            };
            let jobs = ids
                .into_iter()
                .map(|plan| {
                    core.state.lock().unwrap().plan(&plan).waiting = None;
                    Job::Backup { plan, full }
                })
                .collect();
            finish(&core, jobs, out, |r| r.result == "ok" || r.result == "warning")
        }
        Command::Check { plan, all_data, out, progress } => {
            let core = locked_core(&progress)?;
            let plan = plan_id(&core, &plan)?;
            finish(&core, vec![Job::Check { plan, all: all_data }], out, |r| r.result == "ok")
        }
        Command::Tidy { plan, out, progress } => {
            let core = locked_core(&progress)?;
            let plan = plan_id(&core, &plan)?;
            finish(&core, vec![Job::Prune { plan }], out, |r| r.result == "ok")
        }
        Command::Restore { plan, paths, to, original: _, snapshot, conflict, out, progress } => {
            let core = locked_core(&progress)?;
            let plan = plan_id(&core, &plan)?;
            let snaps = core.snapshots(&plan)?;
            let snap = match &snapshot {
                None => snaps.last().ok_or("this plan has no snapshots yet")?,
                Some(id) => {
                    let found: Vec<_> = snaps.iter().filter(|s| s.id.hex().starts_with(&id.to_lowercase())).collect();
                    match found[..] {
                        [s] => s,
                        [] => return Err(Failure::Usage(format!("no snapshot {id}; `keepr snapshots` lists them"))),
                        _ => return Err(Failure::Usage(format!("more than one snapshot starts with {id}: give more of it"))),
                    }
                }
            };
            let items =
                if paths.is_empty() { snap.sources.clone() } else { paths.iter().map(|p| p.trim_end_matches('/').to_string()).collect() };
            let target = match to {
                Some(dir) => {
                    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                    Target::Folder(dir.canonicalize().map_err(|e| e.to_string())?)
                }
                None => Target::Original,
            };
            let conflict = match conflict {
                OnConflict::KeepBoth => Conflict::KeepBoth,
                OnConflict::Replace => Conflict::Replace,
                OnConflict::Skip => Conflict::Skip,
            };
            let job = Job::Restore { plan, snapshot: snap.id.hex(), items, target, conflict };
            finish(&core, vec![job], out, |r| r.result == "ok" || r.result == "warning")
        }
        Command::VerifyRestore { plan, folder } => {
            let core = Core::new(config::data_dir(), Box::new(|_, _| {}), Box::new(|_, _| {}), true)?;
            let plan = plan_id(&core, &plan)?;
            Ok(if verify::run(&plan, &folder)? { 0 } else { 3 })
        }
        Command::RemovePath { plan, path, yes, out, progress } => {
            if !yes {
                return Err(Failure::Usage(format!("this takes {path} out of every snapshot and can't be undone; add --yes to go ahead")));
            }
            let core = locked_core(&progress)?;
            let plan = plan_id(&core, &plan)?;
            finish(&core, vec![Job::RemovePath { plan, path }], out, |r| r.result == "ok")
        }
        Command::RenamePlanFolder { plan } => {
            let core = locked_core(&Progress { quiet: true })?;
            let plan = plan_id(&core, &plan)?;
            println!("{}", core.rename_plan_folder(&plan)?);
            Ok(0)
        }
        Command::ListObjects { destination, out } => list_objects(&destination, out),
        Command::CopyPassword { from, to } => {
            let core = Core::new(config::data_dir(), Box::new(|_, _| {}), Box::new(|_, _| {}), true)?;
            let (from, to) = (plan_id(&core, &from)?, plan_id(&core, &to)?);
            let pw = keychain::get(&keychain::plan_account(&from)).ok_or_else(|| format!("no password for plan {from}"))?;
            keychain::set(&keychain::plan_account(&to), &pw)?;
            Ok(0)
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "keepr", &mut std::io::stdout());
            Ok(0)
        }
    }
}

/// A Core that only reads: it saves nothing, so it can run beside the app.
fn reading_core() -> Res<Core_> {
    Ok(Core::new(config::data_dir(), Box::new(|_, _| {}), Box::new(|_, _| {}), true)?)
}

/// A Core for the jobs that change a backup or the history, with the data folder's lock held
/// exclusively for as long as the process runs: none of them may run while the app does, which
/// rewrites the same files. Progress goes to stderr unless asked not to.
fn locked_core(progress: &Progress) -> Res<Core_> {
    let dir = config::data_dir();
    let lock = config::lock_data_dir(&dir, true)?;
    std::mem::forget(lock);
    let emit: crate::core::Emit = if progress.quiet { Box::new(|_, _| {}) } else { show_progress() };
    Ok(Core::new(dir, emit, Box::new(|t, b| eprintln!("{t}: {b}")), false)?)
}

/// Prints a running job's progress to stderr: one line kept up to date on a terminal, or a line
/// each time the stage changes when stderr goes to a file.
fn show_progress() -> crate::core::Emit {
    let tty = std::io::stderr().is_terminal();
    let last = Mutex::new((String::new(), Instant::now() - Duration::from_secs(1)));
    Box::new(move |what, v| {
        if what != "job" || v.is_null() {
            return;
        }
        let (name, stage) = (v["planName"].as_str().unwrap_or(""), v["stage"].as_str().unwrap_or(""));
        let (read, to_read) = (v["bytesRead"].as_u64().unwrap_or(0), v["bytesToRead"].as_u64().unwrap_or(0));
        let mut last = last.lock().unwrap();
        let line =
            if to_read > 0 { format!("{name}: {stage} {:.0}%", 100.0 * read as f64 / to_read as f64) } else { format!("{name}: {stage}") };
        if tty {
            if last.1.elapsed() >= Duration::from_millis(250) && line != last.0 {
                eprint!("\r\x1b[2K{line}");
                *last = (line, Instant::now());
            }
        } else if stage != last.0 {
            eprintln!("{line}");
            last.0 = stage.to_string();
        }
    })
}

/// Ctrl-C presses: the first stops the job at a safe point, a second quits at once.
static INTERRUPTS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn on_interrupt(_: libc::c_int) {
    if INTERRUPTS.fetch_add(1, Relaxed) > 0 {
        unsafe { libc::_exit(130) };
    }
}

/// Runs the jobs one after another, waits for them, and prints how each went. `good` says which
/// results count as done; any other gives exit status 1, or 130 when Ctrl-C stopped it.
fn finish(core: &Core_, jobs: Vec<Job>, out: Out, good: impl Fn(&Run) -> bool) -> Res<i32> {
    unsafe { libc::signal(libc::SIGINT, on_interrupt as *const () as libc::sighandler_t) };
    core.start();
    let before = core.state.lock().unwrap().history.len();
    for j in jobs {
        core.enqueue(j);
    }
    let mut told = false;
    while !core.idle() {
        // Stopped, each job that starts is stopped too, so the ones still queued end at once.
        if INTERRUPTS.load(Relaxed) > 0 {
            if !told {
                eprintln!("\nkeepr: stopping after the current file (Ctrl-C again to quit now)");
                told = true;
            }
            if let Some(j) = core.status() {
                core.cancel(&j.id);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if std::io::stderr().is_terminal() {
        eprint!("\r\x1b[2K");
    }
    let runs: Vec<Run> = core.state.lock().unwrap().history[before..].to_vec();
    if out.json {
        print_json(&runs.iter().map(RunOut::from).collect::<Vec<_>>());
    } else {
        let name = |id: &str| core.config.lock().unwrap().plan(id).map(|p| p.name.clone()).unwrap_or(id.to_string());
        for r in &runs {
            println!("{}: {} {}: {}", name(&r.plan), r.kind, r.result, r.message);
        }
    }
    Ok(if runs.iter().all(&good) {
        0
    } else if INTERRUPTS.load(Relaxed) > 0 {
        130
    } else {
        1
    })
}

/// A plan by its id or its name, any case.
fn plan_id(core: &Arc<Core>, given: &str) -> Res<String> {
    let cfg = core.config.lock().unwrap();
    if cfg.plan(given).is_some() {
        return Ok(given.to_string());
    }
    let named: Vec<_> = cfg.plans.iter().filter(|p| p.name.eq_ignore_ascii_case(given)).collect();
    match named[..] {
        [p] => Ok(p.id.clone()),
        [] => Err(Failure::Usage(format!("no plan {given}; `keepr plans` lists them"))),
        _ => Err(Failure::Usage(format!(
            "more than one plan is called {given}; use its id: {}",
            named.iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(", ")
        ))),
    }
}

fn print_json<T: Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

/// Rows as columns, each as wide as its widest cell, the last one left ragged.
fn print_table(head: &[&str], rows: &[Vec<String>]) {
    let mut w: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |cells: Vec<&str>| {
        let n = cells.len();
        let s: Vec<String> =
            cells.iter().enumerate().map(|(i, c)| if i + 1 == n { c.to_string() } else { format!("{c:<0$}", w[i]) }).collect();
        println!("{}", s.join("  "));
    };
    line(head.to_vec());
    for r in rows {
        line(r.iter().map(String::as_str).collect());
    }
}

/// A local time from the stored RFC 3339 one, as "2026-10-10 19:08", or "" for none.
fn when(t: Option<&str>) -> String {
    t.and_then(keepr_engine::retention::parse_time).map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

#[derive(Serialize)]
struct RunOut {
    /// The plan, by id or name.
    plan: String,
    kind: String,
    result: String,
    message: String,
    started: String,
    finished: String,
}

impl From<&Run> for RunOut {
    fn from(r: &Run) -> Self {
        RunOut {
            plan: r.plan.clone(),
            kind: r.kind.clone(),
            result: r.result.clone(),
            message: r.message.clone(),
            started: r.started.clone(),
            finished: r.finished.clone(),
        }
    }
}

#[derive(Serialize)]
struct PlanOut {
    id: String,
    name: String,
    enabled: bool,
    destination: String,
    schedule: String,
    sources: Vec<String>,
    encrypted: bool,
}

fn plans(out: Out) -> Res<i32> {
    let core = reading_core()?;
    let cfg = core.config.lock().unwrap().clone();
    let rows: Vec<PlanOut> = cfg
        .plans
        .iter()
        .map(|p| PlanOut {
            id: p.id.clone(),
            name: p.name.clone(),
            enabled: p.enabled,
            destination: cfg.destination(&p.destination).map(|d| d.name.clone()).unwrap_or_default(),
            schedule: crate::schedule_label(p),
            sources: crate::core::source_names(p),
            encrypted: p.encrypted,
        })
        .collect();
    if out.json {
        print_json(&rows);
    } else {
        let t: Vec<Vec<String>> = rows
            .iter()
            .map(|p| {
                let on = if p.enabled { "" } else { " (off)" };
                vec![p.id.clone(), format!("{}{on}", p.name), p.destination.clone(), p.schedule.clone()]
            })
            .collect();
        print_table(&["ID", "NAME", "DESTINATION", "SCHEDULE"], &t);
    }
    Ok(0)
}

#[derive(Serialize)]
struct DestOut {
    id: String,
    name: String,
    kind: &'static str,
    place: String,
}

fn destinations(out: Out) -> Res<i32> {
    let core = reading_core()?;
    let cfg = core.config.lock().unwrap().clone();
    let rows: Vec<DestOut> = cfg
        .destinations
        .iter()
        .map(|d| DestOut {
            id: d.id.clone(),
            name: d.name.clone(),
            kind: match d.place {
                Place::Folder { .. } => "folder",
                Place::Smb(_) => "smb",
                Place::S3(_) => "s3",
            },
            place: places::describe(&d.place),
        })
        .collect();
    if out.json {
        print_json(&rows);
    } else {
        let t: Vec<Vec<String>> = rows.iter().map(|d| vec![d.id.clone(), d.name.clone(), d.kind.into(), d.place.clone()]).collect();
        print_table(&["ID", "NAME", "KIND", "WHERE"], &t);
    }
    Ok(0)
}

#[derive(Serialize)]
struct StatusOut {
    id: String,
    name: String,
    /// "running", "waiting", "failed", "stale", "never", "off" or "ok".
    status: String,
    message: String,
    last_success: Option<String>,
    next_run: Option<String>,
    snapshots: u64,
}

fn status(only: &[String], out: Out) -> Res<i32> {
    let core = reading_core()?;
    let ids = only.iter().map(|p| plan_id(&core, p)).collect::<Res<Vec<_>>>()?;
    let ov = overview_of(&core);
    let rows: Vec<StatusOut> = ov
        .plans
        .iter()
        .filter(|p| ids.is_empty() || ids.contains(&p.id))
        .map(|p| StatusOut {
            id: p.id.clone(),
            name: p.name.clone(),
            status: p.status.clone(),
            message: p.message.clone(),
            last_success: p.last_success.clone(),
            next_run: p.next_run.clone(),
            snapshots: p.snapshots,
        })
        .collect();
    if out.json {
        print_json(&rows);
    } else {
        let t: Vec<Vec<String>> = rows
            .iter()
            .map(|p| {
                vec![p.name.clone(), p.status.clone(), when(p.last_success.as_deref()), when(p.next_run.as_deref()), p.message.clone()]
            })
            .collect();
        print_table(&["PLAN", "STATUS", "LAST BACKUP", "NEXT", "DETAIL"], &t);
    }
    let attention = rows.iter().any(|p| matches!(p.status.as_str(), "failed" | "waiting" | "stale" | "never"));
    Ok(if attention { 1 } else { 0 })
}

fn snapshots(plan: &str, out: Out) -> Res<i32> {
    let core = reading_core()?;
    let plan = plan_id(&core, plan)?;
    let snaps: Vec<crate::browse::SnapInfo> = core.snapshots(&plan)?.iter().map(crate::browse::info).collect();
    if out.json {
        print_json(&snaps);
    } else {
        let t: Vec<Vec<String>> = snaps
            .iter()
            .map(|s| {
                vec![
                    s.id[..8.min(s.id.len())].to_string(),
                    when(Some(&s.time)),
                    s.kind.clone(),
                    s.files.to_string(),
                    human_bytes(s.bytes),
                    human_bytes(s.added_bytes),
                ]
            })
            .collect();
        print_table(&["ID", "TIME", "KIND", "FILES", "SIZE", "ADDED"], &t);
    }
    Ok(0)
}

#[derive(Serialize)]
struct ObjectOut {
    key: String,
    size: u64,
}

fn list_objects(dest: &str, out: Out) -> Res<i32> {
    let core = reading_core()?;
    let cfg = core.config.lock().unwrap().clone();
    let d = cfg
        .destinations
        .iter()
        .find(|d| d.id == dest || d.name.eq_ignore_ascii_case(dest))
        .ok_or_else(|| Failure::Usage(format!("no destination {dest}; `keepr destinations` lists them")))?;
    let Place::S3(b) = &d.place else {
        return Err(Failure::Usage(format!("{} isn't a bucket", d.name)));
    };
    let objects = places::s3_backend(b, None, "").and_then(|r| keepr_engine::backup::Remote::objects(&r).map_err(|e| e.to_string()))?;
    if out.json {
        print_json(&objects.iter().map(|o| ObjectOut { key: o.key.clone(), size: o.size }).collect::<Vec<_>>());
    } else {
        for o in &objects {
            println!("{:>12}  {}", o.size, o.key);
        }
        eprintln!("{} objects, {}", objects.len(), human_bytes(objects.iter().map(|o| o.size).sum()));
    }
    Ok(0)
}
