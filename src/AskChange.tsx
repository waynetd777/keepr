// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A change to a plan that an answer in Ask proposes, as a keepr-change block: shown with what it
// would do (src-tauri/src/ask.rs `check`) before anything changes, then applied with Apply and
// undone with Undo, here or in Activity. Nothing in the answer runs: the block is data.

import { useEffect, useRef, useState } from "react";
import { api, on, type AskChecked } from "./api";
import { bytes, count } from "./format";

/** The answer's text without its keepr-change blocks, and the blocks (parsed, or why not). While
 * it's `streaming`, a block not yet closed is hidden with what follows it. */
export function splitChanges(text: string, streaming = false): { text: string; changes: { raw: unknown; bad?: string }[] } {
  const changes: { raw: unknown; bad?: string }[] = [];
  // The block's JSON may start on the tag's own line.
  const rest = text.replace(/```keepr-change[ \t]*\n?([\s\S]*?)```/g, (_, body: string) => {
    try {
      changes.push({ raw: JSON.parse(body) });
    } catch {
      changes.push({ raw: null, bad: "The suggested change isn't written as Keepr expects, so it can't be applied." });
    }
    return "";
  });
  // A block still being written: hidden until it's complete.
  const open = streaming ? rest.indexOf("```keepr-change") : -1;
  return { text: open >= 0 ? rest.slice(0, open) : rest, changes };
}

export function AskChange({ raw, bad }: { raw: unknown; bad?: string }) {
  const [checked, setChecked] = useState<AskChecked | null>(null);
  const [err, setErr] = useState(bad ?? "");
  const [busy, setBusy] = useState(false);
  // The Activity entry once applied, for Undo; "undone" after, or "gone" once undone in Activity
  // (or no longer undoable).
  const [applied, setApplied] = useState<string | null>(null);
  const wasApplied = useRef(false);

  // Checked once per change: after Apply, a check would only say the plan already has it.
  const key = bad ? "" : JSON.stringify(raw);
  useEffect(() => {
    if (!key || wasApplied.current) return;
    api.askCheck(JSON.parse(key)).then(setChecked, (e) => setErr(String(e)));
  }, [key]);

  // Undo is offered only while Keepr can still undo it: not once Activity has.
  useEffect(() => {
    if (!applied || applied === "undone" || applied === "gone") return;
    const id = applied;
    const look = () =>
      void api.askUndoable().then(
        (ids) => !ids.includes(id) && setApplied((a) => (a === id ? "gone" : a)),
        () => {},
      );
    look();
    return on("changed", look);
  }, [applied]);

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
                {l.before && l.after ? (
                  <>
                    <span className="was">{l.before}</span> → <b>{l.after}</b>
                  </>
                ) : l.after ? (
                  <b>{l.after}</b>
                ) : (
                  <span className="was">{l.before}</span>
                )}
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
          Keeps {checked.snapshots[2].toLocaleString()} of the {checked.snapshots[0].toLocaleString()} snapshots there now
          {checked.snapshots[1] < checked.snapshots[0] ? ` (the settings now keep ${checked.snapshots[1].toLocaleString()})` : ""}.
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
        ) : applied === "gone" ? (
          <span className="muted">Undone in Activity.</span>
        ) : applied ? (
          <>
            <span className="muted">Applied. It's in Activity too.</span>
            <button
              className="btn small"
              disabled={busy}
              title="Put the plan back as it was"
              onClick={() => {
                setBusy(true);
                setErr("");
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
                .then(
                  (id) => {
                    wasApplied.current = true;
                    setApplied(id);
                  },
                  (e) => setErr(String(e)),
                )
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
