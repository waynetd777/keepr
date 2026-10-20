// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Open an existing backup: one already at a destination, made on another Mac or by a plan
// deleted since. Choose the place it's on (the source), pick the backup there, and it becomes a
// plan, turned off, so its snapshots can be restored here.

import { useState } from "react";
import { api, type FoundBackup } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import { DestIcon, Sheet } from "./ui";
import { AddDestination } from "./Destinations";
import { ago } from "./format";
import { MACHINE } from "./platform";

/** Opens the sheet (App holds it), from wherever it's offered. */
export const openBackup = () => window.dispatchEvent(new Event("keepr:open-backup"));

export function OpenBackupSheet({ onClose }: { onClose: () => void }) {
  const { ov, cfg, go } = useApp();
  // The source chosen: a destination, the one already set up for that place if there is one.
  const [source, setSource] = useState("");
  const [found, setFound] = useState<FoundBackup[] | null>(null);
  const [folder, setFolder] = useState<string | null>(null);
  const [pw, setPw] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [choosing, setChoosing] = useState(false);

  const d = ov?.destinations.find((x) => x.id === source);
  const free = found?.filter((b) => !b.plan) ?? [];
  const chosen = free.find((b) => b.folder === folder);
  const ok = !!chosen && (!chosen.encrypted || pw.length > 0) && !busy;

  // By the place itself, not as it's shown: buckets of one name at different services show alike.
  const where = (id: string) => {
    const pl = cfg?.destinations.find((x) => x.id === id)?.place;
    return pl ? JSON.stringify({ ...pl, name: undefined }) : id;
  };
  const chooseSource = (id: string) => {
    const kept = ov?.destinations.find((x) => where(x.id) === where(id))?.id ?? id;
    setSource(kept);
    setFound(null);
    setFolder(null);
    setErr("");
    api.findBackups(kept).then(
      (f) => {
        setFound(f);
        const open = f.filter((b) => !b.plan);
        if (open.length === 1) setFolder(open[0].folder);
      },
      (e) => setErr(String(e)),
    );
  };

  const open = async () => {
    if (!chosen) return;
    setBusy(true);
    setErr("");
    try {
      const id = await api.openBackup(source, chosen.folder, chosen.encrypted ? pw : undefined);
      onClose();
      go({ name: "restore", plan: id });
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };

  return (
    <Sheet
      title="Open an existing backup"
      subtitle={`A Keepr backup made on another ${MACHINE === "Mac" ? "Mac" : "computer"}, or by a plan you deleted. It opens as a plan that's turned off, ready to restore from.`}
      width={560}
      onClose={onClose}
      foot={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={!ok} onClick={open}>
            {busy ? "Opening…" : "Open"}
          </button>
        </>
      }
    >
      <div className="sheet-body">
        <div className="row" style={{ gap: 10 }}>
          {d && <DestIcon kind={d.kind} label={d.kindLabel} />}
          <span className={`grow ellipsis${d ? "" : " muted"}`} style={{ fontWeight: d ? 600 : undefined }}>
            {d ? d.name : "The drive, share, cloud folder or bucket the backup is on."}
          </span>
          <button className={`btn${d ? "" : " primary"}`} onClick={() => setChoosing(true)}>
            {d ? "Change…" : "Choose a source…"}
          </button>
        </div>
        {found && free.length > 0 && (
          <div className="col" style={{ gap: 6 }}>
            {free.map((b) => {
              const on = b.folder === folder;
              return (
                <button
                  key={b.folder}
                  type="button"
                  onClick={() => {
                    setFolder(b.folder);
                    setErr("");
                  }}
                  className="row"
                  style={{
                    gap: 12,
                    padding: "10px 12px",
                    textAlign: "left",
                    borderRadius: 10,
                    border: `1px solid ${on ? "var(--accent)" : "var(--line)"}`,
                    background: on ? "var(--accent-soft)" : "var(--surface)",
                  }}
                >
                  <span className="grow col" style={{ gap: 2, minWidth: 0 }}>
                    <span style={{ fontWeight: 600 }}>{b.name}</span>
                    <span className="small muted ellipsis">
                      {b.folder || "The source's folder itself"} · made {ago(b.created)}
                    </span>
                  </span>
                  {b.encrypted && <Icon name="lock" size={14} />}
                </button>
              );
            })}
          </div>
        )}
        {found && free.length === 0 && d && <NothingToOpen name={d.name} found={found} />}
        {chosen?.encrypted && (
          <label className="field">
            <span>Its password, as set on the computer that made it</span>
            <input
              className="input"
              type="password"
              value={pw}
              autoFocus
              onChange={(e) => setPw(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && ok && open()}
            />
          </label>
        )}
        {err && <span style={{ color: "var(--red)" }}>{err}</span>}
      </div>
      {choosing && (
        <AddDestination
          title="Open a source"
          okLabel="Open"
          subtitle={`Where the backup is: the drive, share, cloud folder or bucket the other ${MACHINE === "Mac" ? "Mac" : "computer"} backed up to.`}
          onClose={() => setChoosing(false)}
          onAdded={(x) => chooseSource(x.id)}
        />
      )}
    </Sheet>
  );
}

/** Why a source has nothing to open: else choosing the wrong folder would go by without a word. */
function NothingToOpen({ name, found }: { name: string; found: FoundBackup[] }) {
  const used = found.flatMap((b) => (b.plan ? [b.plan] : []));
  return (
    <span className="muted">
      {used.length === 0
        ? `There's no Keepr backup in ${name}. Choose the folder the backup is in, or the one holding it.`
        : `The backup in ${name} is already ${used.length === 1 ? `the plan ${used[0]}` : `the plans ${used.join(", ")}`} on this ${MACHINE}.`}
    </span>
  );
}
