// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Numbers and times the way people say them.

export function bytes(n: number | null | undefined): string {
  if (n == null) return "—";
  const u = ["bytes", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let v = n;
  while (v >= 1000 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  if (i === 0) return `${n} ${n === 1 ? "byte" : "bytes"}`;
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}

export function count(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

const hm = (d: Date) => `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;

function dayDiff(d: Date, now = new Date()): number {
  const a = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const b = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  return Math.round((b - a) / 86_400_000);
}

/** "12 min ago", "3 hours ago", "yesterday", "Thursday", "3 March". */
export function ago(iso: string | null | undefined): string {
  if (!iso) return "never";
  const d = new Date(iso);
  const s = (Date.now() - d.getTime()) / 1000;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  const days = dayDiff(d);
  if (days === 0) return `${Math.round(s / 3600)} hour${Math.round(s / 3600) === 1 ? "" : "s"} ago`;
  if (days === 1) return "yesterday";
  if (days < 7) return DAYS[d.getDay()];
  return longDate(iso);
}

/** "Today 14:00", "Yesterday 02:00", "Thu 24 14:00", "3 Mar 2025 14:00". */
export function when(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const days = dayDiff(d);
  if (days === 0) return `Today ${hm(d)}`;
  if (days === 1) return `Yesterday ${hm(d)}`;
  if (days === -1) return `Tomorrow ${hm(d)}`;
  if (days > 0 && days < 7) return `${DAYS[d.getDay()].slice(0, 3)} ${d.getDate()}, ${hm(d)}`;
  const year = d.getFullYear() !== new Date().getFullYear() ? ` ${d.getFullYear()}` : "";
  return `${d.getDate()} ${MONTHS[d.getMonth()].slice(0, 3)}${year}, ${hm(d)}`;
}

/** "Today, 14:00", "Saturday 26 September, 14:00": the restore screen's heading. */
export function longWhen(iso: string): string {
  const d = new Date(iso);
  const days = dayDiff(d);
  if (days === 0) return `Today, ${hm(d)}`;
  if (days === 1) return `Yesterday, ${hm(d)}`;
  const year = d.getFullYear() !== new Date().getFullYear() ? ` ${d.getFullYear()}` : "";
  return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]}${year}, ${hm(d)}`;
}

export function longDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const year = d.getFullYear() !== new Date().getFullYear() ? ` ${d.getFullYear()}` : "";
  return `${d.getDate()} ${MONTHS[d.getMonth()]}${year}`;
}

/** When the next backup is: "15:00", "tomorrow 02:00", "Sunday 02:00", "now". */
export function next(iso: string | null | undefined): string {
  if (!iso) return "when you ask";
  const d = new Date(iso);
  if (d.getTime() <= Date.now() + 30_000) return "now";
  const days = -dayDiff(d);
  if (days === 0) return hm(d);
  if (days === 1) return `tomorrow ${hm(d)}`;
  if (days < 7) return `${DAYS[d.getDay()]} ${hm(d)}`;
  return `${longDate(iso)} ${hm(d)}`;
}

export function duration(secs: number): string {
  if (secs < 60) return `${Math.max(1, Math.round(secs))} s`;
  if (secs < 3600) return `${Math.round(secs / 60)} min`;
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs % 3600) / 60);
  return m ? `${h} h ${m} min` : `${h} h`;
}

export function secondsLeft(secs: number | null): string {
  if (secs == null) return "";
  if (secs < 60) return "less than a minute left";
  return `about ${duration(secs)} left`;
}

/** A path inside the home folder written from ~. Only a whole folder name matches, so with home
 *  /Users/wayne, /Users/wayned stays as it is. */
export function tilde(path: string, home: string): string {
  if (!home) return path;
  return path === home || path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}

export function dayLabel(iso: string): { short: string; today: boolean } {
  const d = new Date(iso);
  const days = dayDiff(d);
  return { short: days === 0 ? "Today" : `${DAYS[d.getDay()].slice(0, 3)} ${d.getDate()}`, today: days === 0 };
}

export function dayKey(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}
