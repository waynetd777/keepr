// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Settings: opening at login, notifications, when to warn about a backup that has stopped, and
// the appearance.

import { useEffect, useState } from "react";
import { api } from "./api";
import { useApp } from "./context";
import { Seg, Switch, useAct } from "./ui";

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
  useEffect(() => {
    api.loginItem().then(setLogin);
    api.version().then(setVer);
  }, []);
  if (!cfg) return null;
  const s = cfg.settings;
  const save = (patch: Partial<typeof s>) =>
    act(async () => {
      await api.saveSettings({ ...s, ...patch });
      await refresh();
    });
  return (
    <div className="content col" style={{ gap: 18, maxWidth: 860 }}>
      <h1>Settings</h1>
      <section className="card" style={{ overflow: "hidden" }}>
        <div style={{ marginTop: -1 }}>
          <Row
            title="Open at Login"
            sub={
              login[0]
                ? "Keepr starts in the menu bar when you log in, so backups run on time."
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
                ["system", "Match the Mac"],
                ["light", "Light"],
                ["dark", "Dark"],
              ]}
            />
          </Row>
        </div>
      </section>
      <span className="small faint">
        Keepr {ver[0]} (build {ver[1]}). Keepr needs Full Disk Access (System Settings › Privacy &amp; Security) to back up every folder and
        to read a still copy of the startup disk.
        <br />
        Free software under the GNU General Public License, version 3 or later.
      </span>
    </div>
  );
}
