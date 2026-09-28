// The Rust side's commands and events, typed. Shapes match src-tauri/src (serde, camelCase).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Smb = { server: string; share: string; folder: string; user: string; name?: string | null };
export type Place = { kind: "folder"; path: string; name?: string | null } | ({ kind: "smb" } & Smb);

export type Destination = { id: string; name: string; place: Place; disconnectAfter: boolean };

export type Every = "minutes15" | "hourly" | "daily" | "weekly" | "manual";
export type Often = "weekly" | "monthly" | "never";

export type Retention = { allHours: number; dailyDays: number; weeklyWeeks: number; monthlyMonths: number; keepDeletedDays: number };

export type Plan = {
  id: string;
  name: string;
  enabled: boolean;
  sources: Place[];
  destination: string;
  folder: string;
  schedule: { every: Every; at: string; weekday: number };
  retention: Retention;
  excludes: string[];
  gitignore: boolean;
  skipCloudOnly: boolean;
  maxFileSize: number;
  encrypted: boolean;
  fullEvery: Often;
  checkEvery: Often;
  conditions: { catchUp: boolean; minBattery: number; noHotspot: boolean; limitMbps: number };
};

export type Settings = { notifyFailures: boolean; notifySuccess: boolean; staleDays: number };
export type Config = { destinations: Destination[]; plans: Plan[]; settings: Settings };

export type Day = { added: number; failed: boolean; ran: boolean; count: number };
export type PlanSummary = {
  id: string;
  name: string;
  enabled: boolean;
  sources: string;
  destination: string;
  destinationId: string;
  schedule: string;
  encrypted: boolean;
  status: "running" | "waiting" | "failed" | "stale" | "never" | "off" | "ok";
  message: string;
  lastSuccess: string | null;
  lastAttempt: string | null;
  nextRun: string | null;
  repoBytes: number;
  versionsBytes: number;
  snapshots: number;
  oldest: string | null;
  days: Day[];
  recoveryUnsaved: boolean;
  lastChanged: number;
  icon: "folder" | "photos" | "notes";
};
export type DestSummary = { id: string; name: string; kind: "folder" | "smb"; place: string; connection: "connected" | "on demand" | "missing"; free: number | null; total: number | null; keeprBytes: number; plans: string[] };
export type JobStatus = {
  id: string;
  kind: "backup" | "full" | "restore" | "check" | "prune" | "remove";
  plan: string;
  planName: string;
  stage: string;
  filesSeen: number;
  filesToRead: number;
  filesRead: number;
  bytesToRead: number;
  bytesRead: number;
  sentBytes: number;
  dupBytes: number;
  current: string;
  startedAt: string;
  paused: boolean;
  rate: number;
  etaSecs: number | null;
  queued: number;
};
export type Queued = { id: string; plan: string; kind: string };
export type Overview = { plans: PlanSummary[]; destinations: DestSummary[]; storedBytes: number; versionsBytes: number; job: JobStatus | null; queued: Queued[] };

export type Run = {
  id: string;
  plan: string;
  kind: "backup" | "full" | "check" | "prune" | "restore" | "remove";
  started: string;
  finished: string;
  result: "ok" | "warning" | "failed" | "cancelled" | "waiting";
  message: string;
  files: number;
  changed: number;
  readBytes: number;
  addedBytes: number;
  storedBytes: number;
  dupBytes: number;
  log?: string[];
};

export type SnapInfo = { id: string; time: string; kind: string; files: number; bytes: number; changed: number; addedBytes: number };
export type Entry = { name: string; path: string; kind: "file" | "dir" | "link"; size: number; mtime: number; tag: "" | "new" | "changed" | "deleted"; versions: number; items: number };
export type Version = { snapshot: string; time: string; size: number; mtime: number; keptIn: number };
export type Found = { plan: string; planName: string; snapshot: string; time: string; gone: boolean; entry: Entry };
export type Target = { kind: "original" } | { kind: "folder"; path: string };
export type Conflict = "keepBoth" | "replace" | "skip";
export type Tested = { ok: boolean; message: string; free: number | null; total: number | null; mbps: number | null };
export type Line = { kind: "same" | "added" | "removed" | "gap"; text: string; old: number | null; new: number | null };
export type Comparison = { currentExists: boolean; identical: boolean; text: boolean; added: number; removed: number; lines: Line[]; backupSize: number; currentSize: number };

export const api = {
  overview: () => invoke<Overview>("overview"),
  config: () => invoke<Config>("get_config"),
  runLog: (id: string) => invoke<string[]>("run_log", { id }),
  history: (limit = 50, offset = 0, filter: "all" | "problems" | "restores" = "all") => invoke<Run[]>("history", { offset, limit, filter }),
  defaultExcludes: () => invoke<string[]>("default_excludes"),
  savePlan: (plan: Plan, password?: string) => invoke<Plan>("save_plan", { plan, password: password ?? null }),
  deletePlan: (id: string) => invoke<void>("delete_plan", { id }),
  setPlanEnabled: (id: string, enabled: boolean) => invoke<void>("set_plan_enabled", { id, enabled }),
  startNewBackup: (id: string) => invoke<void>("start_new_backup", { id }),
  saveDestination: (dest: Destination, password?: string) => invoke<Destination>("save_destination", { dest, password: password ?? null }),
  deleteDestination: (id: string) => invoke<void>("delete_destination", { id }),
  suggestName: (place: Place, except?: string) => invoke<string>("suggest_name", { place, except: except ?? null }),
  testPlace: (place: Place, password?: string) => invoke<Tested>("test_place", { place, password: password ?? null }),
  discoverServers: () => invoke<string[]>("discover_servers"),
  listShares: (server: string, user: string, password?: string) => invoke<string[]>("list_shares", { server, user, password: password ?? null }),
  hasPassword: (accountKind: "plan" | "smb", id: string, user?: string) => invoke<boolean>("has_password", { accountKind, id, user: user ?? null }),
  recoveryKey: (id: string) => invoke<string | null>("recovery_key", { id }),
  recoverySaved: (id: string) => invoke<void>("recovery_saved", { id }),
  changePassword: (id: string, current: string, next: string) => invoke<void>("change_password", { id, current, new: next }),
  backUp: (plan: string, full = false) => invoke<string>("back_up", { plan, full }),
  backUpAll: () => invoke<void>("back_up_all"),
  restore: (plan: string, snapshot: string, items: string[], target: Target, conflict: Conflict) => invoke<string>("restore", { plan, snapshot, items, target, conflict }),
  checkNow: (plan: string, all: boolean) => invoke<string>("check_now", { plan, all }),
  removeSourceData: (plan: string, source: Place) => invoke<string>("remove_source_data", { plan, source }),
  jobStatus: () => invoke<JobStatus | null>("job_status"),
  cancel: (id = "") => invoke<void>("job_cancel", { id }),
  pause: (paused: boolean) => invoke<void>("job_pause", { paused }),
  pauseHour: () => invoke<void>("pause_hour"),
  snapshots: (plan: string) => invoke<SnapInfo[]>("snapshots", { plan }),
  listDir: (plan: string, snapshot: string, path: string, showDeleted: boolean) => invoke<Entry[]>("list_dir", { plan, snapshot, path, showDeleted }),
  versions: (plan: string, path: string) => invoke<Version[]>("file_versions", { plan, path }),
  search: (plan: string, snapshot: string, query: string) => invoke<Entry[]>("search_snapshot", { plan, snapshot, query }),
  searchEverywhere: (query: string) => invoke<{ found: Found[]; missed: string[] }>("search_everywhere", { query }),
  quickLook: (plan: string, snapshot: string, path: string) => invoke<void>("quick_look", { plan, snapshot, path }),
  compare: (plan: string, snapshot: string, path: string) => invoke<Comparison>("compare", { plan, snapshot, path }),
  /** macOS's folder panel, one kept and reused (a new one takes ~26 s on macOS 27). */
  chooseFolders: (title: string, multiple = false, start?: string) => invoke<string[]>("choose_folders", { title, multiple, start: start ?? null }),
  version: () => invoke<[string, string]>("app_version"),
  home: () => invoke<string>("home_dir"),
  loginItem: () => invoke<[boolean, boolean]>("login_item"),
  setLoginItem: (on: boolean) => invoke<boolean>("set_login_item", { on }),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  scene: () => invoke<string | null>("scene"),
  showMain: (screen?: string) => invoke<void>("show_main_window", { screen: screen ?? null }),
  quit: () => invoke<void>("quit"),
};

export function on<T>(event: string, f: (payload: T) => void): () => void {
  let un: UnlistenFn | undefined;
  let gone = false;
  listen<T>(event, (e) => f(e.payload)).then((u) => (gone ? u() : (un = u)));
  return () => {
    gone = true;
    un?.();
  };
}
