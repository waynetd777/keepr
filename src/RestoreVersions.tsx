// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Restore screen's side pane: the picked file's versions and what can be done with one, or
// what the picked folder held.

import type { Entry, Version } from "./api";
import { Icon } from "./icons";
import { onlyOnMac } from "./RestoreFiles";
import { bytes, tilde, when } from "./format";

const ONE_ONLY = "For one item at a time: this is one of several selected";

/** Remove from backup and Show in Finder, for the picked file or folder. */
function PickedActions({
  dir,
  multi,
  onMac,
  remove,
  reveal,
}: {
  dir: boolean;
  multi: boolean;
  onMac: boolean;
  remove: () => void;
  reveal: () => void;
}) {
  return (
    <>
      <button
        className="btn small danger"
        onClick={remove}
        disabled={multi}
        title={
          multi
            ? ONE_ONLY
            : dir
              ? "Take this folder out of every snapshot and free its space"
              : "Take this out of every snapshot and free its space"
        }
      >
        <Icon name="trash" size={13} />
        Remove from backup…
      </button>
      <button
        className="btn small"
        disabled={!onMac || multi}
        onClick={reveal}
        title={multi ? ONE_ONLY : dir ? "Show the folder on your Mac in Finder" : "Show the file on your Mac in its folder"}
      >
        <Icon name="folder" size={13} />
        Show in Finder
      </button>
    </>
  );
}

export function VersionsPane(p: {
  picked: Entry | null;
  home: string;
  /// The snapshot shown, so its own version can say so.
  snapId: string;
  versions: Version[] | null;
  ver: number;
  setVer: (i: number) => void;
  /// The picked item is one of several chosen: what acts on one item at a time is greyed out.
  multi: boolean;
  /// Its original is on the Mac, to be shown in Finder.
  onMac: boolean;
  looking: boolean;
  quickLook: () => void;
  compare: (v: Version) => void;
  restore: (v: Version) => void;
  remove: () => void;
  reveal: () => void;
}) {
  const { picked, home, versions, ver, multi } = p;
  const version = versions?.[ver];
  const actions = (dir: boolean) => <PickedActions dir={dir} multi={multi} onMac={p.onMac} remove={p.remove} reveal={p.reveal} />;
  return (
    <aside
      aria-label="Versions"
      style={{
        width: 340,
        flexShrink: 0,
        borderLeft: "1px solid var(--line)",
        background: "var(--sunk)",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div
        style={{
          padding: "18px 20px 14px",
          display: "flex",
          flexDirection: "column",
          gap: 4,
          borderBottom: "1px solid var(--line)",
        }}
      >
        <span className="caps">{picked?.kind === "dir" ? "Folder" : "Versions"}</span>
        <span style={{ fontSize: 17, fontWeight: 600, wordBreak: "break-word" }}>
          {picked ? (picked.name.startsWith("/") ? tilde(picked.name, home) : picked.name) : "Pick a file"}
        </span>
        {picked && <span className="small muted">{tilde(picked.path.split("/").slice(0, -1).join("/") || "/", home)}</span>}
      </div>
      {picked?.kind === "file" && (
        <>
          <div
            style={{
              flexGrow: 1,
              minHeight: 0,
              overflow: "auto",
              padding: "8px 10px",
              display: "flex",
              flexDirection: "column",
              gap: 2,
            }}
          >
            {!versions && (
              <span className="small muted" style={{ padding: 10 }}>
                Finding versions…
              </span>
            )}
            {versions?.map((v, i) => (
              <button key={v.snapshot} className={`version${i === ver ? " on" : ""}`} onClick={() => p.setVer(i)}>
                <span className="dot" />
                <span className="grow col" style={{ gap: 1 }}>
                  <span style={{ fontWeight: 600 }}>{when(v.time)}</span>
                  <span className="tiny muted">
                    {v.snapshot === p.snapId
                      ? "In this snapshot"
                      : i === 0
                        ? "Newest version"
                        : `Kept in ${v.keptIn + 1} snapshot${v.keptIn ? "s" : ""}`}
                  </span>
                </span>
                <span className="mono muted" style={{ fontSize: 11 }}>
                  {bytes(v.size)}
                </span>
              </button>
            ))}
            {versions && (
              <div className="small faint" style={{ padding: "8px 10px" }}>
                {versions.length === 1 ? "This is the only version." : `${versions.length} versions in all.`}
              </div>
            )}
          </div>
          <div style={{ padding: "12px 16px", borderTop: "1px solid var(--line)", display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button
              className="btn"
              title={multi ? ONE_ONLY : "Quick Look (Space)"}
              disabled={!version || p.looking || multi}
              onClick={p.quickLook}
            >
              {p.looking ? <span className="dot spin" style={{ width: 12, height: 12 }} /> : <Icon name="eye" size={14} stroke={1.9} />}
              {p.looking ? "Opening…" : "Quick Look"}
            </button>
            <button
              className="btn"
              title={multi ? ONE_ONLY : "Show what changed from the file on your Mac"}
              disabled={!version || multi}
              onClick={() => version && p.compare(version)}
            >
              Compare with current
            </button>
            <button
              className="btn"
              disabled={!version || multi}
              title={multi ? ONE_ONLY : undefined}
              onClick={() => version && p.restore(version)}
            >
              Restore this version
            </button>
          </div>
        </>
      )}
      {picked && picked.kind !== "dir" && !onlyOnMac(picked) && (
        <div style={{ padding: "0 16px 12px", display: "flex", gap: 8 }}>{actions(false)}</div>
      )}
      {picked?.kind === "dir" && !onlyOnMac(picked) && (
        <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 10, color: "var(--ink2)", lineHeight: 1.5 }}>
          <span>
            {picked.name.startsWith("/") ? tilde(picked.name, home) : picked.name} held {bytes(picked.size)} at this snapshot
            {picked.items ? `, ${picked.items.toLocaleString()} items at its top` : ""}.
          </span>
          <span>Tick the folder to restore everything in it as it was then. Pick a file to see its versions.</span>
          <div className="row" style={{ gap: 8 }}>
            {actions(true)}
          </div>
        </div>
      )}
    </aside>
  );
}
