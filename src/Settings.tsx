// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Settings: opening at login, notifications, when to warn about a backup that has stopped, and
// the appearance.

import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type HelperStatus, type Settings as SettingsData } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import { Seg, Switch, useAct } from "./ui";
import { isMac, LOG_IN, MACHINE, MENU_BAR } from "./platform";

/** Keepr's website. The same as `WEBSITE` in src-tauri/src/about.rs, which the About panel and
 * Help › Keepr Website use (a test there keeps them the same). */
export const WEBSITE = "https://keepr.davies.co.za/";

export type Theme = "system" | "light" | "dark";

export function applyTheme(t: Theme) {
  if (t === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
}

export function savedTheme(): Theme {
  try {
    return (localStorage.getItem("theme") as Theme) || "system";
  } catch {
    return "system";
  }
}

/** Keepr Helper, on Windows: a still copy of the drive for each backup, and only the folders
 *  that changed looked at, as macOS gives Keepr itself. Needs an administrator once. */
function HelperRow() {
  const act = useAct();
  const [st, setSt] = useState<HelperStatus | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.helperStatus().then(setSt);
  }, []);
  if (!st) return null;
  const set = async (on: boolean) => {
    setBusy(true);
    const r = await act(() => api.helperSet(on));
    setBusy(false);
    setSt(r ?? (await api.helperStatus()));
  };
  const sub = st.running
    ? st.outdated
      ? "Keepr Helper is from another version of Keepr. Update it so backups can use it again."
      : "Each backup reads a still copy of the drive (a shadow copy), and looks only in folders that changed (the drive's change journal)."
    : st.installed
      ? "Keepr Helper is installed but not running. Install it again to start it."
      : "Read each backup from a still copy of the drive, so files that change during it are copied as they were at one moment, and look only in folders that changed. Installs Keepr Helper, a small Windows service; Windows asks for an administrator once.";
  return (
    <Row title="Still copies and the change journal" sub={sub}>
      <div className="row" style={{ gap: 8 }}>
        {st.running && st.outdated && (
          <button className="btn" disabled={busy} onClick={() => set(true)}>
            Update
          </button>
        )}
        {st.installed && !st.running && (
          <button className="btn" disabled={busy} onClick={() => set(true)}>
            Install again
          </button>
        )}
        <Switch label="Still copies and the change journal" disabled={busy} on={st.running} onChange={(v) => set(v)} />
      </div>
    </Row>
  );
}

const Row = ({ title, sub, children }: { title: string; sub?: string; children: React.ReactNode }) => (
  <div className="row" style={{ gap: 16, padding: "16px 20px", borderTop: "1px solid var(--line)" }}>
    <div className="grow col" style={{ gap: 2 }}>
      <span style={{ fontWeight: 600 }}>{title}</span>
      {sub && <span className="small muted">{sub}</span>}
    </div>
    {children}
  </div>
);

export default function Settings() {
  const { cfg, refresh } = useApp();
  const act = useAct();
  const [login, setLogin] = useState<[boolean, boolean]>([false, false]);
  const [ver, setVer] = useState<[string, string]>(["", ""]);
  const [theme, setTheme] = useState<Theme>(savedTheme());
  // What the switches show while a change is being saved, so they move at once and a second
  // change starts from the first rather than from the settings before it.
  const [shown, setShown] = useState<SettingsData | null>(null);
  const saving = useRef(Promise.resolve());
  const pending = useRef(0);
  useEffect(() => {
    api.loginItem().then(setLogin);
    api.version().then(setVer);
  }, []);
  if (!cfg) return null;
  const s = shown ?? cfg.settings;
  // Saves run one after another, so an earlier one never lands last and undoes a later one.
  const save = (patch: Partial<SettingsData>) => {
    const next = { ...s, ...patch };
    setShown(next);
    pending.current++;
    saving.current = saving.current
      .then(() =>
        act(async () => {
          await api.saveSettings(next);
          await refresh();
        }),
      )
      .then(() => {
        if (--pending.current === 0) setShown(null);
      });
  };
  return (
    <div className="content col" style={{ gap: 18, maxWidth: 860 }}>
      <h1>Settings</h1>
      <section className="card" style={{ overflow: "hidden" }}>
        <div style={{ marginTop: -1 }}>
          <Row
            title="Open at Login"
            sub={
              login[0]
                ? `Keepr starts in the ${MENU_BAR} when you ${LOG_IN}, so backups run on time.`
                : "Works in the installed app, not in a development build."
            }
          >
            <Switch
              label="Open at Login"
              disabled={!login[0]}
              on={login[1]}
              onChange={async (v) => setLogin([login[0], await api.setLoginItem(v)])}
            />
          </Row>
          <Row
            title="Offer Getting started when Keepr opens"
            sub="A welcome that opens the help's step-by-step guides. They're always in the help: press ? and click Getting started."
          >
            <Switch label="Offer Getting started when Keepr opens" on={s.offerGuide !== false} onChange={(v) => save({ offerGuide: v })} />
          </Row>
          {!isMac && <HelperRow />}
          <Row title="Tell me when a backup fails" sub="A notification when a backup, check or restore doesn't finish.">
            <Switch label="Notify failures" on={s.notifyFailures} onChange={(v) => save({ notifyFailures: v })} />
          </Row>
          <Row title="Tell me when a backup finishes" sub="A notification for every completed backup.">
            <Switch label="Notify successes" on={s.notifySuccess} onChange={(v) => save({ notifySuccess: v })} />
          </Row>
          <Row title="Warn when a plan hasn't backed up for" sub="Once a day, until it backs up again.">
            <select className="input" value={s.staleDays} onChange={(e) => save({ staleDays: Number(e.target.value) })}>
              {[1, 2, 3, 5, 7, 14].map((d) => (
                <option key={d} value={d}>
                  {d} day{d === 1 ? "" : "s"}
                </option>
              ))}
            </select>
          </Row>
          <Row
            title="Ask"
            sub={`An AI already signed in on this ${MACHINE} answers questions about your backups, with Keepr's figures and folder names, never your files. Off, nothing is sent.`}
          >
            <Switch label="Ask" on={s.ask !== false} onChange={(v) => save({ ask: v })} />
          </Row>
          <Row title="Website" sub="What Keepr does, and the latest version to download.">
            <div className="row" style={{ gap: 8 }}>
              <button className="btn" title="Opens keepr.davies.co.za in your browser" onClick={() => openUrl(WEBSITE)}>
                <Icon name="globe" size={14} stroke={2} />
                Website
              </button>
              {/* Windows has no app menu to hold it. */}
              {!isMac && (
                <button className="btn" title="Looks for a newer Keepr and offers to install it" onClick={() => api.checkForUpdates()}>
                  Check for Updates…
                </button>
              )}
            </div>
          </Row>
          <Row title="Appearance">
            <Seg
              label="Appearance"
              value={theme}
              onChange={(t) => {
                setTheme(t);
                applyTheme(t);
                try {
                  localStorage.setItem("theme", t);
                } catch {
                  /* a private window */
                }
              }}
              options={[
                ["system", `Match ${isMac ? "the Mac" : "Windows"}`],
                ["light", "Light"],
                ["dark", "Dark"],
              ]}
            />
          </Row>
        </div>
      </section>
      <span className="small faint">
        Keepr {ver[0]} (build {ver[1]}).{" "}
        {isMac
          ? "Keepr needs Full Disk Access (System Settings › Privacy & Security) to back up every folder and to read a still copy of the startup disk."
          : "Keepr backs up what your Windows account can read; other accounts' folders and Windows' own need an administrator."}
        <br />
        Free software under the GNU General Public License, version 3 or later.
      </span>
    </div>
  );
}
