// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What Keepr is set up to do (destinations and plans, in config.json) and what it has done
//! (state.json: each plan's last runs, and the history). Both live in
//! ~/Library/Application Support/Keepr/ and are written whole, atomically.

use keepr_engine::retention::Retention;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("KEEPR_DATA") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/Keepr")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Smb {
    pub server: String,
    pub share: String,
    /// Inside the share, "/" for its top.
    #[serde(default)]
    pub folder: String,
    pub user: String,
    /// What people call it; empty or missing for the default (see places::default_name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A bucket on Amazon S3 or a service that speaks its API. The secret key is in the Keychain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct S3 {
    /// "https://s3.eu-west-1.amazonaws.com", or another service's address.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    /// A folder inside the bucket; empty for its top.
    #[serde(default)]
    pub prefix: String,
    pub access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Place {
    /// A folder on this Mac or on a drive.
    Folder {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Smb(Smb),
    /// Only a destination: nothing is backed up from a bucket.
    S3(S3),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub place: Place,
    /// Disconnect a share Keepr connected once it's done with it.
    #[serde(default = "yes")]
    pub disconnect_after: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Every {
    Minutes15,
    Hourly,
    Daily,
    Weekly,
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub every: Every,
    /// "HH:MM" for daily and weekly.
    #[serde(default = "two_am")]
    pub at: String,
    /// 0 = Sunday, for weekly.
    #[serde(default)]
    pub weekday: u32,
}

fn two_am() -> String {
    "02:00".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Conditions {
    /// Run a missed backup as soon as the Mac wakes or the destination comes back.
    #[serde(default = "yes")]
    pub catch_up: bool,
    /// Back up while on battery power at all; off, a backup waits for mains power.
    #[serde(default = "yes")]
    pub on_battery: bool,
    /// On battery, wait while it's below this percentage (0: never wait).
    #[serde(default = "twenty")]
    pub min_battery: u32,
    /// Wait while on a network macOS calls expensive (a personal hotspot).
    #[serde(default = "yes")]
    pub no_hotspot: bool,
    /// MB/s, 0 for no limit.
    #[serde(default)]
    pub limit_mbps: u32,
}

fn twenty() -> u32 {
    20
}

impl Default for Conditions {
    fn default() -> Conditions {
        Conditions { catch_up: true, on_battery: true, min_battery: 20, no_hotspot: true, limit_mbps: 0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Often {
    Weekly,
    Monthly,
    Never,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub sources: Vec<Place>,
    pub destination: String,
    /// The repository's folder inside the destination, fixed when the plan is made.
    pub folder: String,
    pub schedule: Schedule,
    #[serde(default)]
    pub retention: Retention,
    #[serde(default)]
    pub excludes: Vec<String>,
    #[serde(default = "yes")]
    pub gitignore: bool,
    #[serde(default = "yes")]
    pub skip_cloud_only: bool,
    /// Leave out what apps have marked for backups to skip, as Time Machine does.
    #[serde(default = "yes")]
    pub skip_marked: bool,
    /// Bytes, 0 for no limit.
    #[serde(default)]
    pub max_file_size: u64,
    #[serde(default)]
    pub encrypted: bool,
    /// Re-read every file (a full backup) this often.
    #[serde(default = "weekly")]
    pub full_every: Often,
    /// Check the stored data this often: a 5% sample weekly, all of it monthly.
    #[serde(default = "weekly")]
    pub check_every: Often,
    #[serde(default)]
    pub conditions: Conditions,
    /// A command run before each backup (for example, one that downloads a device's own backups
    /// into a folder this plan backs up). Run with /bin/zsh -lc, so ~ and the usual PATH work.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before: String,
    /// If the command fails: carry on and mark the backup with a warning (false), or don't back up (true).
    #[serde(default)]
    pub before_must_succeed: bool,
}

fn weekly() -> Often {
    Often::Weekly
}

pub fn default_excludes() -> Vec<String> {
    [
        "node_modules/",
        "target/",
        ".DS_Store",
        "*.tmp",
        "~/Library/Caches",
        ".Trash/",
        ".Spotlight-V100/",
        ".fseventsd/",
        ".Trashes/",
        ".DocumentRevisions-V100/",
        ".TemporaryItems/",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "yes")]
    pub notify_failures: bool,
    #[serde(default)]
    pub notify_success: bool,
    /// Warn when a plan hasn't completed a backup for this many days.
    #[serde(default = "three")]
    pub stale_days: u32,
}

fn three() -> u32 {
    3
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { notify_failures: true, notify_success: false, stale_days: 3 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub destinations: Vec<Destination>,
    #[serde(default)]
    pub plans: Vec<Plan>,
    #[serde(default)]
    pub settings: Settings,
}

impl Config {
    pub fn plan(&self, id: &str) -> Option<&Plan> {
        self.plans.iter().find(|p| p.id == id)
    }
    pub fn destination(&self, id: &str) -> Option<&Destination> {
        self.destinations.iter().find(|d| d.id == id)
    }
}

/// One thing Keepr did: a backup, check, tidy-up or restore.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub plan: String,
    /// "backup", "full", "check", "prune" or "restore".
    pub kind: String,
    pub started: String,
    pub finished: String,
    /// "ok", "warning", "failed", "cancelled" or "waiting".
    pub result: String,
    pub message: String,
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub changed: u64,
    #[serde(default)]
    pub read_bytes: u64,
    #[serde(default)]
    pub added_bytes: u64,
    #[serde(default)]
    pub stored_bytes: u64,
    #[serde(default)]
    pub dup_bytes: u64,
    /// What happened, step by step, while the run is going; written to logs/<id>.log when it
    /// ends (so state.json stays small) and read back only when Activity expands the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
}

impl Run {
    /// Adds a line to the run's log, stamped with the time.
    pub fn note(&mut self, line: impl AsRef<str>) {
        if self.log.len() < 400 {
            self.log.push(format!("{}  {}", chrono::Local::now().format("%H:%M:%S"), line.as_ref()));
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlanState {
    #[serde(default)]
    pub last_attempt: Option<String>,
    #[serde(default)]
    pub last_success: Option<String>,
    #[serde(default)]
    pub last_full: Option<String>,
    #[serde(default)]
    pub last_check: Option<String>,
    #[serde(default)]
    pub last_prune: Option<String>,
    /// The repository has been made; from then on a missing one is an error, not a fresh start.
    #[serde(default)]
    pub created: bool,
    /// Bytes the repository takes, as of the last backup or prune.
    #[serde(default)]
    pub repo_bytes: u64,
    /// What all the snapshots hold together, as the files' sizes add up.
    #[serde(default)]
    pub versions_bytes: u64,
    #[serde(default)]
    pub snapshots: u64,
    #[serde(default)]
    pub oldest: Option<String>,
    /// Why the plan is waiting, when it is ("Archive SSD isn't connected").
    #[serde(default)]
    pub waiting: Option<String>,
    /// The day the stale-backup warning was last sent.
    #[serde(default)]
    pub stale_warned: Option<String>,
    /// macOS's file-system event id noted just before the last backup, and that backup's
    /// snapshot: the next backup asks what changed since.
    #[serde(default)]
    pub fs_event: Option<(u64, String)>,
    /// Set when the recovery key hasn't been saved yet.
    #[serde(default)]
    pub recovery_unsaved: bool,
}

/// A destination's space, as last seen connected.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DestState {
    pub free: u64,
    pub total: u64,
    pub checked: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default)]
    pub plans: std::collections::HashMap<String, PlanState>,
    #[serde(default)]
    pub destinations: std::collections::HashMap<String, DestState>,
    #[serde(default)]
    pub history: Vec<Run>,
}

pub const HISTORY_MAX: usize = 5000;

impl State {
    pub fn plan(&mut self, id: &str) -> &mut PlanState {
        self.plans.entry(id.to_string()).or_default()
    }
    /// Adds a run; returns the runs that fell off the end, whose logs can go.
    pub fn record(&mut self, run: Run) -> Vec<Run> {
        self.history.push(run);
        if self.history.len() > HISTORY_MAX {
            let extra = self.history.len() - HISTORY_MAX;
            return self.history.drain(..extra).collect();
        }
        vec![]
    }
}

pub fn read<T: serde::de::DeserializeOwned + Default>(dir: &Path, name: &str) -> T {
    match std::fs::read(dir.join(name)) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
            eprintln!("{name}: {e}; starting afresh, the old file kept as {name}.bad");
            let _ = std::fs::rename(dir.join(name), dir.join(format!("{name}.bad")));
            T::default()
        }),
        Err(_) => T::default(),
    }
}

pub fn write<T: Serialize>(dir: &Path, name: &str, v: &T) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())
}

pub fn new_id() -> String {
    keepr_engine::Id::random().hex()[..12].to_string()
}

/// "Documents & Projects" → "Documents-Projects".
pub fn folder_name(name: &str, id: &str) -> String {
    let mut s: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-');
    format!("{} {}", if s.is_empty() { "Plan" } else { s }, &id[..6])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes_and_survives_junk() {
        let t = tempfile::tempdir().unwrap();
        let mut c = Config::default();
        c.destinations.push(Destination {
            id: "d".into(),
            name: "keep-nas".into(),
            place: Place::Smb(Smb {
                server: "keep-nas.local".into(),
                share: "Backups".into(),
                folder: "/Keepr".into(),
                user: "wayne".into(),
                name: None,
            }),
            disconnect_after: true,
        });
        write(t.path(), "config.json", &c).unwrap();
        let back: Config = read(t.path(), "config.json");
        assert_eq!(back.destinations[0].place, c.destinations[0].place);
        std::fs::write(t.path().join("config.json"), b"{not json").unwrap();
        let back: Config = read(t.path(), "config.json");
        assert!(back.destinations.is_empty());
        assert!(t.path().join("config.json.bad").exists());
        assert_eq!(folder_name("Documents & Projects", "abcdef123"), "Documents-Projects abcdef");
    }
}
