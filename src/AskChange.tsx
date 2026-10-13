// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A change to a plan that an answer in Ask proposes, as a keepr-change block: shown with what it
// would do (src-tauri/src/ask.rs `check`) before anything changes, then applied with Apply and
// undone with Undo, here or in Activity. Nothing in the answer runs: the block is data.

import { useEffect, useState } from "react";
import { api, type AskChecked } from "./api";
import { bytes, count } from "./format";

/** The answer's text without its keepr-change blocks, and the blocks (parsed, or why not). */
export function splitChanges(text: string): { text: string; changes: { raw: unknown; bad?: string }[] } {
  const changes: { raw: unknown; bad?: string }[] = [];
  const rest = text.replace(/```keepr-change\s*\n([\s\S]*?)```/g, (_, body: string) => {
    try {
      changes.push({ raw: JSON.parse(body) });
    } catch {
      changes.push({ raw: null, bad: "The suggested change isn't written as Keepr expects, so it can't be applied." });
    }
    return "";
  });
  // A block still being written: hidden until it's complete.
  const open = rest.indexOf("```keepr-change");
  return { text: open >= 0 ? rest.slice(0, open) : rest, changes };
}

export function AskChange({ raw, bad }: { raw: unknown; bad?: string }) {
  const [checked, setChecked] = useState<AskChecked | null>(null);
  const [err, setErr] = useState(bad ?? "");
  const [busy, setBusy] = useState(false);
  // The Activity entry once applied, for Undo; "undone" after.
  const [applied, setApplied] = useState<string | null>(null);

  useEffect(() => {
    if (bad) return;
    api.askCheck(raw).then(setChecked, (e) => setErr(String(e)));
  }, [raw, bad]);

  if (err && !checked) return <div className="ask-change err">{err}</div>;
  if (!checked) return <div className="ask-change muted">Working out what this change would do…</div>;
  const lo = checked.leftOut;
  return (
    <div className="ask-change" aria-label={`Suggested change to ${checked.planName}`}>
      <h4>Suggested change to {checked.planName}</h4>
      {checked.why && <span>{checked.why}</span>}
      <table>
        <tbody>
          {checked.lines.map((l) => (
            <tr key={l.what}>
              <td className="muted nowrap">{l.what}</td>
              <td>
                <div className="was">{l.before}</div>
                <div>{l.after}</div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {lo && (
        <span>
          {lo.files
            ? `Leaves out ${count(lo.files, "file")}, ${bytes(lo.bytes)} in the latest backup, from now on.`
            : "Nothing in the latest backup matches the new rules."}
          {lo.folders.length > 0 && <span className="muted"> {lo.folders.map(([p, b]) => `${p} (${bytes(b)})`).join(", ")}.</span>}{" "}
          <span className="muted">What's stored already goes at a tidy-up once no kept snapshot holds it.</span>
        </span>
      )}
      {checked.snapshots && (
        <span>
          Keeps {checked.snapshots[1].toLocaleString()} of the {checked.snapshots[0].toLocaleString()} snapshots kept now.
        </span>
      )}
      {checked.warnings.map((w) => (
        <span key={w} className="warn">
          {w}
        </span>
      ))}
      {err && <span className="err">{err}</span>}
      <div className="row" style={{ gap: 8 }}>
        {applied === "undone" ? (
          <span className="muted">Undone: the plan is as it was.</span>
        ) : applied ? (
          <>
            <span className="muted">Applied. It's in Activity too.</span>
            <button
              className="btn small"
              disabled={busy}
              title="Put the plan back as it was"
              onClick={() => {
                setBusy(true);
                api
                  .askUndo(applied)
                  .then(
                    () => setApplied("undone"),
                    (e) => setErr(String(e)),
                  )
                  .finally(() => setBusy(false));
              }}
            >
              Undo
            </button>
          </>
        ) : (
          <button
            className="btn small primary"
            disabled={busy}
            title={`Save this change to ${checked.planName}, as the plan editor would`}
            onClick={() => {
              setBusy(true);
              setErr("");
              api
                .askApply(raw)
                .then(setApplied, (e) => setErr(String(e)))
                .finally(() => setBusy(false));
            }}
          >
            Apply
          </button>
        )}
      </div>
    </div>
  );
}
