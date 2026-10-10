// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Restore screen's footer: what's chosen, where it goes and what happens to a file already
// there, and the Restore button; or, while a restore of this plan runs or waits, its progress.

import type { Conflict, JobStatus, Queued } from "./api";
import { Icon } from "./icons";
import { PlanProgress, Seg } from "./ui";
import { bytes, tilde, when } from "./format";

const FOOT: React.CSSProperties = {
  height: 72,
  flexShrink: 0,
  display: "flex",
  alignItems: "center",
  gap: 18,
  padding: "0 32px",
  background: "var(--surface)",
  borderTop: "1px solid var(--line)",
};

export function RestoreFooter(p: {
  /// When the snapshot shown was taken.
  time: string;
  restoring: JobStatus | null;
  queued?: Queued;
  count: number;
  total: number;
  dest: "original" | "folder";
  setDest: (d: "original" | "folder") => void;
  folder: string;
  chooseFolder: () => void;
  conflict: Conflict;
  setConflict: (c: Conflict) => void;
  home: string;
  restore: () => void;
}) {
  const { restoring, queued, count, dest, folder } = p;
  if (restoring || queued) {
    return (
      <footer style={FOOT}>
        <div className="col" style={{ gap: 2, minWidth: 150 }}>
          <span style={{ fontWeight: 600 }}>{restoring ? "Restoring" : "Restore waiting"}</span>
          <span className="small muted">
            {restoring
              ? `${restoring.filesRead.toLocaleString()} of ${restoring.filesToRead.toLocaleString()} files · ${bytes(restoring.bytesRead)} of ${bytes(restoring.bytesToRead)}`
              : "Starts when the current job finishes"}
          </span>
        </div>
        <div className="grow">
          <PlanProgress job={restoring} queued={restoring ? undefined : queued} />
        </div>
      </footer>
    );
  }
  return (
    <footer style={FOOT}>
      <div className="col" style={{ gap: 2, minWidth: 150 }}>
        <span style={{ fontWeight: 600 }}>{count === 0 ? "Nothing selected" : `${count} item${count === 1 ? "" : "s"} selected`}</span>
        <span className="small muted">
          {bytes(p.total)} · from {when(p.time)}
        </span>
      </div>
      <div style={{ width: 1, height: 36, background: "var(--line)" }} />
      <div className="col" style={{ gap: 4 }}>
        <span className="tiny faint">Restore to</span>
        <Seg
          label="Restore to"
          value={dest}
          // Another folder asks which, the first time.
          onChange={(v) => (v === "folder" && !folder ? p.chooseFolder() : p.setDest(v))}
          options={[
            ["original", "Original location"],
            ["folder", "Another folder…"],
          ]}
        />
      </div>
      {dest === "folder" && folder && (
        <div className="col" style={{ gap: 4, minWidth: 0 }}>
          <span className="tiny faint">Folder</span>
          <button className="btn" onClick={p.chooseFolder} style={{ maxWidth: 260 }}>
            <Icon name="folder" size={14} />
            <span className="ellipsis">{tilde(folder, p.home)}</span>
          </button>
        </div>
      )}
      <label className="col" style={{ gap: 4 }}>
        <span className="tiny faint">{dest === "original" ? "If a file is already there" : "If the folder has one"}</span>
        <select className="input" value={p.conflict} onChange={(e) => p.setConflict(e.target.value as Conflict)}>
          <option value="keepBoth">Keep both (add “restored”)</option>
          <option value="replace">Replace it</option>
          <option value="skip">Skip it</option>
        </select>
      </label>
      <span className="grow" />
      <span className="small muted" style={{ maxWidth: 230, textAlign: "right", lineHeight: 1.4 }}>
        {dest === "original"
          ? "Each item goes back where it was. Folders that no longer exist are made again."
          : "Items keep their folders inside the folder you choose."}
      </span>
      <button className="btn primary big" disabled={count === 0} onClick={p.restore}>
        <Icon name="restore" size={15} stroke={2.2} />
        {count === 0 ? "Restore" : `Restore ${count} item${count === 1 ? "" : "s"}`}
      </button>
    </footer>
  );
}
