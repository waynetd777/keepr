// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ask: runs an AI coding CLI already installed and signed in on this Mac — Claude Code, Codex,
//! Antigravity or GitHub Copilot — non-interactively, and streams its answer back to the window
//! as events. It has no tools: the message carries the scan, and the app itself deletes nothing.
//! A follow-up resumes the CLI's session. The model id says which CLI answers.

mod antigravity;
mod claude;
mod codex;
mod copilot;

use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

const SYSTEM: &str = "You are an adviser inside Keepr, a macOS backup app. Each backup plan backs up some folders to a destination (a drive, a NAS share, a synced cloud folder or an S3 bucket) on a schedule, keeping versions as snapshots under its retention rules; data is de-duplicated and compressed, so what a backup stores is far less than the files it holds. \
The user's message carries what Keepr knows: each plan's settings (what it backs up and leaves out, its schedule, what it keeps), its status, its biggest folders with what grew this week, the space they take stored, folder names that repeat, folders whose old versions are still stored, and the recent runs with their results. Folders are named; files are not, and you see no file contents. For each folder you get its file types by size, when its newest file changed, how much changed in 30 days, and whether its project's .gitignore lists it. \
Answer the question asked. For questions about Keepr itself (what a screen, figure or setting means, how to do something), answer from the user guide below. For space questions, say in plain language what each big folder is likely to be, from its name, its file types and how it changes, and whether it needs backing up: folders tools rebuild (node_modules, .venv, build output, caches) don't, and neither does what can be downloaded again; photos, documents and anything made by hand do. Rank by space saved against risk, and say when a folder's purpose is unclear rather than guess. For a failed or slow run, explain it from the run's message and log in plain language, and say what to do. \
In a backup app the remedy is to leave something out of a plan, or keep fewer versions, not to delete files from the Mac; say so if the user asks about deleting. Leaving out a folder stops new versions being made; what's stored goes at the next tidy-up only once no kept snapshot holds it. \
When a change to a plan would help and the user wants it, propose it as a fenced block with the language keepr-change holding one JSON object, after your explanation, one block per plan. Its keys: plan (the plan's id), why (one sentence), and only those you change of addExcludes and removeExcludes (lists of rules), gitignore, skipCloudOnly and skipMarked (true to leave those files out), maxFileSize (bytes, 0 for no limit), retention (any of allHours, dailyDays, weeklyWeeks, monthlyMonths (0 keeps monthly versions forever), keepDeletedDays) and schedule (every: minutes15, hourly, daily, weekly or manual; at: HH:MM; weekday: 0 for Sunday). A rule is a name or pattern (*.tmp), a folder name ending in / (node_modules/), or a path starting with / or ~/ (~/Library/Caches). The app shows the user what the change would do and asks before applying it; you can change nothing else (not the folders backed up, the destination or encryption), and nothing changes until the user applies it. \
You have no tools and no files to read: answer from the message alone, run nothing, and never claim to have checked or changed anything. \
Write tightly: short paragraphs or bullets, headings only when they help, paths as written in the message.";

/// The in-app help's pages (src/help/), in the order the drawer shows them (src/guides.ts): the
/// whole guide, about 20 KB, so Ask can explain the app's figures as the guide does and answer
/// how anything in Tidemark works (Help's own button sends that question here).
/// Each page's name (its file in src/help/ without `.md`) and its text.
macro_rules! help_pages {
    ($($name:literal),* $(,)?) => {
        [$(($name, include_str!(concat!("../../../src/help/", $name, ".md")))),*]
    };
}
const HELP: [(&str, &str); 17] = help_pages![
    "guide-start",
    "guide-watch",
    "guide-restore",
    "guide-ask",
    "overview",
    "plan",
    "sizemap",
    "restore",
    "search",
    "activity",
    "destinations",
    "storage",
    "settings",
    "ask",
    "menubar",
    "keyboard",
    "command-line",
];

/// The system prompt: SYSTEM, then the whole guide, built once.
fn system() -> &'static str {
    static PROMPT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let guide: Vec<&str> = HELP.iter().map(|(_, p)| p.trim()).collect();
        format!("{SYSTEM}\n\nThe app's user guide:\n\n{}", guide.join("\n\n"))
    });
    &PROMPT
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Chunk {
    chat_id: String,
    text: String,
}

/// Says what the CLI is doing before its answer starts ("Claude is thinking it through"), as
/// an `ask-status` event; the panel shows it in place of "Thinking".
fn status(app: &AppHandle, chat_id: &str, text: &str) {
    let _ = tauri::Emitter::emit(app, "ask-status", Chunk { chat_id: chat_id.to_string(), text: text.to_string() });
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Done {
    chat_id: String,
    session_id: Option<String>,
    text: String,
    error: Option<String>,
}

/// The CLI processes under way, by chat, so a chat can be stopped and nothing outlives the app.
#[derive(Default)]
pub struct Running {
    children: Mutex<Vec<(String, u32)>>,
}

impl Running {
    fn remove(&self, pid: u32) {
        if let Ok(mut ch) = self.children.lock() {
            ch.retain(|(_, p)| *p != pid);
        }
    }
    /// A CLI is answering: an update's restart would stop it (updates.rs waits).
    pub fn busy(&self) -> bool {
        self.children.lock().is_ok_and(|ch| !ch.is_empty())
    }
    pub fn kill_all(&self) {
        if let Ok(ch) = self.children.lock() {
            for (_, pid) in ch.iter() {
                kill_group(*pid);
            }
        }
    }
}

/// The CLIs start helpers of their own, so the whole process group goes.
fn kill_group(pid: u32) {
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
    }
}

/// Where a CLI is installed: the usual places first, then the login shell's PATH.
fn find(name: &str, extra: &[&str]) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut places: Vec<String> = vec![format!("{home}/.local/bin/{name}")];
    places.extend(extra.iter().map(|p| format!("{home}/{p}")));
    places.extend([format!("/opt/homebrew/bin/{name}"), format!("/usr/local/bin/{name}")]);
    for p in places {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let out = Command::new("/bin/zsh").args(["-lc", &format!("command -v {name}")]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty() && PathBuf::from(&s).is_file()).then(|| PathBuf::from(s))
}

fn version(bin: &PathBuf) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.trim_end_matches('.').to_string())
}

#[derive(Serialize)]
pub struct Model {
    id: String,
    name: String,
}

#[derive(Serialize)]
pub struct Cli {
    path: Option<String>,
    version: Option<String>,
    models: Vec<Model>,
}

#[derive(Serialize)]
pub struct Status {
    claude: Cli,
    codex: Cli,
    antigravity: Cli,
    copilot: Cli,
}

/// Which CLIs are installed, and the models each offers. With no models anywhere, the window
/// disables Ask and says why.
#[tauri::command]
pub async fn assistant_status() -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let cli = |bin: Option<PathBuf>, models: Vec<Model>| Cli {
            version: bin.as_ref().and_then(version),
            models: if bin.is_some() { models } else { Vec::new() },
            path: bin.map(|p| p.to_string_lossy().to_string()),
        };
        Status {
            claude: cli(claude::find(), claude::models()),
            codex: cli(codex::find(), codex::models()),
            antigravity: cli(antigravity::find(), antigravity::models()),
            copilot: cli(copilot::find(), copilot::models()),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

fn spawn(
    cmd: &mut Command,
    running: &Running,
    chat_id: &str,
    name: &str,
) -> Result<(Child, ChildStdout, std::thread::JoinHandle<String>), String> {
    use std::os::unix::process::CommandExt;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {name}: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take();
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    if let Ok(mut ch) = running.children.lock() {
        ch.push((chat_id.to_string(), child.id()));
    }
    Ok((child, stdout, err))
}

fn finish(mut child: Child, running: &Running) -> bool {
    let ok = child.wait().is_ok_and(|s| s.success());
    running.remove(child.id());
    ok
}

/// The error for a CLI that answered nothing: its stderr if it failed with any, else a plain line.
fn silent(name: &str, ok: bool, stderr: std::thread::JoinHandle<String>) -> String {
    let msg = stderr.join().unwrap_or_default();
    if msg.trim().is_empty() || ok {
        format!("{name} stopped without answering")
    } else {
        msg.trim().to_string()
    }
}

/// Puts a question to the CLI the model id names; the answer arrives as `ask-chunk` events while
/// it is written and `ask-done` closes it, with the session to resume for a follow-up.
#[tauri::command]
pub async fn ask(
    app: AppHandle,
    running: tauri::State<'_, Arc<Running>>,
    chat_id: String,
    prompt: String,
    model: String,
    session: Option<String>,
) -> Result<(), String> {
    let cli = if model.starts_with(antigravity::PREFIX) {
        "agy"
    } else if model.starts_with(copilot::PREFIX) {
        "copilot"
    } else if model.starts_with("claude") {
        "claude"
    } else {
        "codex"
    };
    // Each CLI gets an empty folder of its own to run in, so there is nothing to read.
    let cwd = app.path().app_data_dir().map_err(|e| e.to_string())?.join(cli);
    std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
    let running = Arc::clone(&running);
    let name = match cli {
        "agy" => "Antigravity",
        "copilot" => "GitHub Copilot",
        "claude" => "Claude Code",
        _ => "Codex",
    };
    status(&app, &chat_id, &format!("Starting {name}"));
    match cli {
        "agy" => antigravity::ask(app, running, cwd, chat_id, prompt, model, session),
        "copilot" => copilot::ask(app, running, cwd, chat_id, prompt, model, session),
        "claude" => claude::ask(app, running, cwd, chat_id, prompt, model, session),
        _ => codex::ask(app, running, cwd, chat_id, prompt, model, session),
    }
}

/// Stops a chat's answer; the window hears `ask-done` as the process ends.
#[tauri::command]
pub fn ask_cancel(running: tauri::State<'_, Arc<Running>>, chat_id: String) {
    if let Ok(ch) = running.children.lock() {
        for (_, pid) in ch.iter().filter(|(id, _)| id == &chat_id) {
            kill_group(*pid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole guide, every page of it, follows the instructions.
    #[test]
    fn prompt_carries_the_whole_guide() {
        let p = system();
        assert!(p.starts_with(SYSTEM));
        for (_, page) in HELP {
            assert!(p.contains(page.trim()));
        }
        // Help's sections on the app's own figures among them.
        for title in ["Space used", "What grew this week"] {
            assert!(p.contains(&format!("## {title}\n")), "missing {title}");
        }
    }

    /// Every page in src/help/ is in HELP, and nothing else: a page added to the drawer is
    /// added here too, or Ask wouldn't know it.
    #[test]
    fn help_lists_every_page() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/help");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .expect("src/help is readable")
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "md").then(|| p.file_stem()?.to_str().map(String::from))?
            })
            .collect();
        files.sort();
        let mut listed: Vec<String> = HELP.iter().map(|(n, _)| n.to_string()).collect();
        listed.sort();
        assert_eq!(listed, files);
    }
}
