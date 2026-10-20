// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The platform the page runs on, in the words and keys it uses: Keepr runs on macOS and on
// Windows, and says Finder or File Explorer, menu bar or notification area, Keychain or
// Credential Manager, ⌘ or Ctrl, as each does. Paths come from the app in macOS's form on both
// (src-tauri/engine/src/paths.rs: on Windows "/" is the system drive), and are shown here as the
// system writes them.

export const isMac = navigator.userAgent.includes("Mac");
export const isWindows = navigator.userAgent.includes("Windows");

/** Whether the shortcut key (⌘ or Ctrl) is held. */
export const cmdKey = (e: { metaKey: boolean; ctrlKey: boolean }) => (isMac ? e.metaKey : e.ctrlKey);

const MODS: Record<string, string> = { "⌃": "Ctrl", "⌘": "Ctrl", "⌥": "Alt", "⇧": "Shift" };
const ORDER = ["Ctrl", "Alt", "Shift"];
const NAMES: Record<string, string> = { "+": "Plus", "−": "Minus", "↑": "Up", "↓": "Down", "←": "Left", "→": "Right", "⌫": "Backspace" };

/** Shortcuts as they're written on macOS ("⇧⌘R", "⌘-click", "⌘K"), in Windows' words there
 *  ("Ctrl+Shift+R", "Ctrl-click", "Ctrl+K"); unchanged on macOS. */
export function shortcut(s: string): string {
  if (isMac) return s;
  return s
    .replace(/([⌃⌥⇧⌘]+)([^\s)·⌃⌥⇧⌘-])?/g, (_, mods: string, key?: string) => {
      const names = ORDER.filter((n) => [...mods].some((m) => MODS[m] === n));
      return [...names, ...(key ? [NAMES[key] ?? key.toUpperCase()] : [])].join("+");
    })
    .replace(/[↩⏎]/g, "Enter")
    .replace(/\bReturn\b/g, "Enter");
}

/** What the app's words call the computer and its parts, on macOS and on Windows. */
export const OS = isMac ? "macOS" : "Windows";
export const MACHINE = isMac ? "Mac" : "PC";
export const FILES_APP = isMac ? "Finder" : "File Explorer";
export const MENU_BAR = isMac ? "menu bar" : "notification area";
/** Where Keepr keeps passwords. */
export const KEYCHAIN = isMac ? "Keychain" : "Credential Manager";
/** Starting a session: macOS logs in, Windows signs in. */
export const LOG_IN = isMac ? "log in" : "sign in";
/** Looking at a file from a backup: Quick Look on macOS; on Windows it opens in its own app. */
export const LOOK = isMac ? "Quick Look" : "Open";

/// The system drive, for showing paths on Windows (src-tauri/src/lib.rs system_drive).
let systemDrive = "C:";
export function setSystemDrive(d: string | undefined) {
  if (d) systemDrive = d;
}

/** A path as the system writes it: as it is on macOS; on Windows with its drive and backslashes,
 *  "/Users/alex" as "C:\Users\alex", "D:/Photos" as "D:\Photos" and "//nas/Backups" as
 *  "\\nas\Backups". Anything else (an s3:// address) is left as it is. */
export function shown(path: string): string {
  if (isMac || !isAbsolute(path)) return path;
  const p = path.startsWith("/") && !path.startsWith("//") ? systemDrive + path : path;
  return p.replace(/\//g, "\\");
}

/** An app path rather than a name or an s3:// address: "/…", or on Windows a drive's ("D:/…"). */
export function isAbsolute(path: string): boolean {
  return path.startsWith("/") || (!isMac && /^[A-Za-z]:\//.test(path));
}

/** The last part of an app path ("Documents" for "/Users/alex/Documents"; a drive's letter, "D:",
 *  for its root). */
export function baseName(path: string): string {
  const t = path.replace(/\/+$/, "");
  return t.split("/").pop() || t || path;
}

/** The folder an app path is in ("/Users/alex"; "D:/" for "D:/Photos"), "" at a root. */
export function parentPath(path: string): string {
  const t = path.replace(/\/+$/, "");
  const i = t.lastIndexOf("/");
  if (i < 0) return "";
  if (i === 0) return "/";
  return i === 2 && t[1] === ":" ? t.slice(0, 3) : t.slice(0, i);
}

/** A share as the system writes it: smb://server/share/folder on macOS, \\server\share\folder on
 *  Windows. */
export function smbAddress(server: string, share: string, folder = ""): string {
  const f = folder.replace(/^\/+|\/+$/g, "");
  const s = share.replace(/^\/+|\/+$/g, "");
  return isMac ? `smb://${server}/${s}${f ? `/${f}` : ""}` : `\\\\${server}\\${s}${f ? `\\${f.replace(/\//g, "\\")}` : ""}`;
}

/** The window's drag areas, macOS only: there the title bar is hidden and the toolbar stands in
 *  for it. Windows has its own title bar, and treats a drag area as part of it, where nothing
 *  inside scrolls or takes a click. */
export const dragRegion = (): Record<string, string> => (isMac ? { "data-tauri-drag-region": "" } : {});
