// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Search: a name in any backup, every plan and every snapshot. Each result opens in Restore at
// the newest snapshot that has it, deleted files included.

import { useEffect, useState } from "react";
import { api, type Found } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import { bytes, tilde, when } from "./format";

export default function Search() {
  const { screen, go, home } = useApp();
  const query = screen.name === "search" ? screen.query : "";
  const [found, setFound] = useState<Found[] | null>(null);
  const [missed, setMissed] = useState<string[]>([]);
  useEffect(() => {
    setFound(null);
    api.searchEverywhere(query).then(
      (r) => {
        setFound(r.found);
        setMissed(r.missed);
      },
      (e) => {
        setFound([]);
        setMissed([String(e)]);
      },
    );
  }, [query]);
  // Each plan's search stops at 300, so the note names the plans that reached it.
  const perPlan = new Map<string, { name: string; n: number }>();
  for (const f of found ?? []) {
    const p = perPlan.get(f.plan) ?? { name: f.planName, n: 0 };
    p.n++;
    perPlan.set(f.plan, p);
  }
  const capped = [...perPlan.values()].filter((p) => p.n >= 300).map((p) => p.name);
  return (
    <div className="content col" style={{ gap: 16 }}>
      <div className="col" style={{ gap: 4 }}>
        <h1>“{query}”</h1>
        <span className="muted" style={{ fontSize: 14 }}>
          {found === null
            ? "Looking in every backup…"
            : found.length === 0
              ? "Nothing by that name in any backup."
              : `${found.length.toLocaleString()} found in your backups${capped.length ? ` (only the first 300 from ${capped.join(", ")})` : ""}. Pick one to see its versions and restore it.`}
        </span>
      </div>
      {missed.length > 0 && (
        <div className="banner">
          <Icon name="warning" size={18} />
          <span className="text">Not searched, because its destination can't be reached right now: {missed.join(", ")}.</span>
        </div>
      )}
      {found && found.length > 0 && (
        <section className="card" style={{ overflow: "hidden" }}>
          <div className="files-head caps" style={{ letterSpacing: "0.04em" }}>
            <span className="grow" style={{ paddingLeft: 26 }}>
              Name
            </span>
            <span style={{ width: 180 }}>Plan</span>
            <span style={{ width: 150 }}>Newest version</span>
            <span style={{ width: 76, textAlign: "right" }}>Size</span>
            <span style={{ width: 76 }} />
          </div>
          {found.map((f) => {
            const parent = f.entry.path.split("/").slice(0, -1).join("/");
            return (
              <div key={f.plan + f.entry.path} className={`file-row${f.gone ? " gone" : ""}`} style={{ height: 44 }}>
                <button
                  className="name"
                  style={{ height: 44 }}
                  onClick={() => go({ name: "restore", plan: f.plan, focus: { snapshot: f.snapshot, path: f.entry.path } })}
                >
                  <span className="grow row" style={{ gap: 10 }}>
                    <Icon
                      name={f.entry.kind === "dir" ? "folder" : "file"}
                      style={{ color: f.entry.kind === "dir" ? "var(--accent)" : "var(--ink2)", flexShrink: 0 }}
                    />
                    <span className="col" style={{ gap: 0, minWidth: 0 }}>
                      <span className="ellipsis" style={{ fontWeight: 600, textDecoration: f.gone ? "line-through" : "none" }}>
                        {f.entry.name}
                      </span>
                      <span className="tiny faint ellipsis">{tilde(parent, home)}</span>
                    </span>
                  </span>
                  <span className="muted ellipsis" style={{ width: 180 }}>
                    {f.planName}
                  </span>
                  <span className="muted nowrap" style={{ width: 150 }}>
                    {when(f.time)}
                  </span>
                  <span className="mono muted" style={{ width: 76, textAlign: "right", fontSize: 11 }}>
                    {f.entry.kind === "file" ? bytes(f.entry.size) : ""}
                  </span>
                  <span style={{ width: 76, display: "flex", justifyContent: "flex-end" }}>
                    {f.gone && <span className="tag deleted">Deleted</span>}
                  </span>
                </button>
              </div>
            );
          })}
        </section>
      )}
    </div>
  );
}
