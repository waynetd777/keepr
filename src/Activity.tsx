// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Activity: what is running now, step by step, and the history of every backup, check, tidy-up
// and restore.

import { Fragment, useCallback, useEffect, useRef, useState } from "react";

const PAGE = 50;
import { api, type Run } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import { AFTER_COMMAND, BEFORE_COMMAND, pct as progressOf, Seg, STOPPING, StopButton } from "./ui";
import { bytes, duration, secondsLeft, tilde, when } from "./format";
import { resultLabel, resultTone } from "./runResult";

const STEPS = ["Look", "Compare", "Pack", "Send", "Confirm"];

function stepOf(stage: string): number {
  // The command before comes ahead of every step, so none is ticked while it runs.
  if (stage === BEFORE_COMMAND) return -1;
  if (stage === "Connecting" || stage === "Taking a still copy") return 0;
  if (stage === "Looking") return 1;
  if (stage === "Reading" || stage === "Sending") return 3;
  if (stage === "Saving") return 4;
  return 5;
}

function Running() {
  const { job, home } = useApp();
  if (!job) return null;
  const isBackup = job.kind === "backup" || job.kind === "full";
  const pct = progressOf(job);
  const step = stepOf(job.stage);
  const verb =
    job.kind === "restore" ? "Restoring from" : job.kind === "check" ? "Checking" : job.kind === "prune" ? "Tidying up" : "Keeping";
  const skipped = job.dupBytes;
  return (
    <>
      <div className="col" style={{ gap: 4 }}>
        <h1>
          {verb} {job.planName}
          <span style={{ color: "var(--accent)" }}>…</span>
        </h1>
        <span className="muted" style={{ fontSize: 14 }}>
          {job.stage === BEFORE_COMMAND || job.stage === AFTER_COMMAND
            ? `${job.stage}. Its output goes in the backup's log.`
            : job.kind === "full"
              ? "A full backup: every file is read again."
              : job.kind === "backup"
                ? "An incremental backup."
                : job.stage}
          {` Started ${when(job.startedAt).replace(/^Today /, "")}.`}
          {isBackup && " You can keep working; Keepr reads a still copy of your files."}
        </span>
      </div>
      <div className="row" style={{ gap: 18, alignItems: "stretch" }}>
        <section className="card grow" style={{ padding: "20px 22px", display: "flex", flexDirection: "column", gap: 18 }}>
          {isBackup && (
            <ol className="steps">
              {STEPS.map((name, i) => {
                const done = i < step;
                const now = i === step || (i === 2 && step === 3);
                const info = [
                  `${job.filesSeen.toLocaleString()} files looked at`,
                  `${job.filesToRead.toLocaleString()} to read · ${bytes(job.bytesToRead)}`,
                  "Split, skip what's kept, compress, encrypt",
                  "Alongside packing",
                  "Write the snapshot last",
                ][i];
                return (
                  <li key={name}>
                    <div className="bar">
                      <div style={{ width: done ? "100%" : now ? `${pct}%` : 0 }} />
                    </div>
                    <div
                      className="row"
                      style={{ gap: 6, fontWeight: 600, color: done ? "var(--ink)" : now ? "var(--accent-text)" : "var(--ink3)" }}
                    >
                      {done && <Icon name="check" size={13} stroke={2.8} />}
                      {name}
                    </div>
                    <span className="small muted" style={{ lineHeight: 1.4 }}>
                      {info}
                    </span>
                  </li>
                );
              })}
            </ol>
          )}
          <div className="col" style={{ gap: 8 }}>
            <div className="row" style={{ alignItems: "baseline", gap: 12 }}>
              <span className="big-number" style={{ fontSize: 40 }}>
                {Math.floor(pct)}%
              </span>
              <span className="mono muted grow">
                {bytes(job.bytesRead)} of {bytes(job.bytesToRead)}
                {job.rate > 0 ? ` · ${bytes(job.rate)}/s` : ""}
              </span>
              <span className="muted">{job.stopping ? STOPPING : job.paused ? "Paused" : secondsLeft(job.etaSecs)}</span>
              <button className="btn" disabled={job.stopping} onClick={() => api.pause(!job.paused)}>
                <Icon name={job.paused ? "play" : "pause"} size={13} stroke={2.4} />
                {job.paused ? "Resume" : "Pause"}
              </button>
              <StopButton job={job} />
            </div>
            <div className="progress">
              <div
                className="soft"
                style={{ width: `${job.bytesToRead ? (100 * Math.min(skipped, job.bytesRead)) / job.bytesToRead : 0}%` }}
              />
              <div
                className="solid"
                style={{ width: `${job.bytesToRead ? (100 * Math.max(0, job.bytesRead - skipped)) / job.bytesToRead : 0}%` }}
              />
            </div>
            {isBackup && (
              <div className="row small muted" style={{ gap: 18 }}>
                <span className="row" style={{ gap: 6 }}>
                  <span style={{ width: 8, height: 8, borderRadius: 2, background: "var(--bar)" }} />
                  Already stored, skipped
                </span>
                <span className="row" style={{ gap: 6 }}>
                  <span style={{ width: 8, height: 8, borderRadius: 2, background: "var(--accent)" }} />
                  New data, sent
                </span>
              </div>
            )}
          </div>
          {job.current && (
            <div className="mono muted ellipsis" style={{ borderTop: "1px solid var(--line)", paddingTop: 12, fontSize: 11 }}>
              {tilde(job.current, home)}
            </div>
          )}
        </section>
        {isBackup && (
          <aside
            className="card"
            style={{ width: 320, flexShrink: 0, padding: "20px 22px", display: "flex", flexDirection: "column", gap: 14 }}
          >
            <span className="caps">This backup so far</span>
            <div className="col" style={{ gap: 2 }}>
              <span className="big-number">{bytes(job.sentBytes)}</span>
              <span className="muted">sent</span>
            </div>
            <div className="col" style={{ gap: 10, borderTop: "1px solid var(--line)", paddingTop: 12 }}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <span className="muted">Files to read</span>
                <span className="mono">{job.filesToRead.toLocaleString()}</span>
              </div>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <span className="muted">Read so far</span>
                <span className="mono">{job.filesRead.toLocaleString()}</span>
              </div>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <span className="muted">Already stored, skipped</span>
                <span className="mono">{bytes(job.dupBytes)}</span>
              </div>
              {job.queued > 0 && (
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <span className="muted">Waiting after this</span>
                  <span className="mono">{job.queued}</span>
                </div>
              )}
            </div>
            <span className="grow" />
            <span className="small faint" style={{ lineHeight: 1.45 }}>
              If the Mac sleeps or the destination drops off, the next backup picks up the work. Nothing half-sent counts as a snapshot.
            </span>
          </aside>
        )}
      </div>
    </>
  );
}

export default function Activity() {
  const { ov, job } = useApp();
  const [runs, setRuns] = useState<Run[]>([]);
  const [filter, setFilter] = useState<"all" | "problems" | "restores">("all");
  // Fifty at a time: the next fifty load when the end of the list scrolls into view. Always read
  // from the top, so a refresh keeps what has been scrolled to and a late answer can't clobber a
  // newer one.
  const [more, setMore] = useState(true);
  const count = useRef(0);
  const gen = useRef(0);
  const busy = useRef(false);
  const end = useRef<HTMLDivElement>(null);
  // Why the history couldn't be read, shown in place of it.
  const [failed, setFailed] = useState("");
  const load = useCallback(
    async (n: number) => {
      const g = ++gen.current;
      busy.current = true;
      try {
        const page = await api.history(n, 0, filter);
        if (g !== gen.current) return;
        count.current = page.length;
        setRuns(page);
        setMore(page.length === n);
        setFailed("");
      } catch (e) {
        if (g !== gen.current) return;
        // No more pages are asked for until the next refresh tries again.
        setMore(false);
        setFailed(String(e));
      } finally {
        // Only the newest load holds the list busy, so a stale one ending can't free it early.
        if (g === gen.current) busy.current = false;
      }
    },
    [filter],
  );
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [logs, setLogs] = useState<Record<string, string[]>>({});
  const toggle = (id: string) => {
    const s = new Set(open);
    if (s.has(id)) s.delete(id);
    else {
      s.add(id);
      // A log that can't be read says why, rather than Loading… for ever.
      if (!logs[id])
        api.runLog(id).then(
          (l) => setLogs((m) => ({ ...m, [id]: l })),
          (e) => setLogs((m) => ({ ...m, [id]: [`Couldn't read this run's log: ${e}`] })),
        );
    }
    setOpen(s);
  };
  // The first page when the filter changes; what's already shown, again, whenever something
  // changes (a run finishes, or the half-minute refresh). That's when the plans are read again,
  // not on every progress tick of a running job, which rebuilds the overview many times a second.
  const shownFilter = useRef(filter);
  useEffect(() => {
    if (shownFilter.current !== filter) {
      shownFilter.current = filter;
      count.current = 0;
    }
    load(Math.max(PAGE, count.current));
  }, [ov?.plans, filter, load]);
  useEffect(() => {
    const el = end.current;
    if (!el) return;
    const io = new IntersectionObserver((e) => e[0].isIntersecting && more && !busy.current && load(count.current + PAGE), {
      rootMargin: "200px",
    });
    io.observe(el);
    return () => io.disconnect();
  }, [more, runs, load]);
  const nameOf = (id: string) => ov?.plans.find((p) => p.id === id)?.name ?? "A deleted plan";
  const kindName: Record<string, string> = {
    backup: "Incremental",
    full: "Full re-read",
    check: "Check",
    prune: "Tidy up",
    restore: "Restore",
    remove: "Remove a folder",
  };
  return (
    <div className="content col" style={{ gap: 18 }}>
      {job ? <Running /> : <h1>Activity</h1>}
      <section className="card" style={{ display: "flex", flexDirection: "column", overflow: "hidden", flexShrink: 0 }}>
        <div className="row" style={{ height: 46, padding: "0 20px", borderBottom: "1px solid var(--line)" }}>
          <h2 className="grow">History</h2>
          <Seg
            label="Filter"
            value={filter}
            onChange={setFilter}
            options={[
              ["all", "All"],
              ["problems", "Problems"],
              ["restores", "Restores"],
            ]}
          />
        </div>
        {failed && runs.length === 0 ? (
          <div className="empty">Couldn't read the history: {failed}</div>
        ) : runs.length === 0 ? (
          <div className="empty">Nothing here yet.</div>
        ) : (
          <table className="history">
            <thead>
              <tr>
                <th>When</th>
                <th>Plan</th>
                <th>Kind</th>
                <th>Files</th>
                <th style={{ textAlign: "right" }}>Sent</th>
                <th style={{ textAlign: "right" }}>Took</th>
                <th>Result</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((r) => {
                const secs = (new Date(r.finished).getTime() - new Date(r.started).getTime()) / 1000;
                const isOpen = open.has(r.id);
                return (
                  <Fragment key={r.id}>
                    <tr onClick={() => toggle(r.id)} style={{ cursor: "default" }} title={isOpen ? "Hide the log" : "Show the log"}>
                      <td className="muted nowrap">
                        <span className="row" style={{ gap: 6 }}>
                          <button
                            className={`disc${isOpen ? " open" : ""}`}
                            aria-label={isOpen ? "Hide the log" : "Show the log"}
                            aria-expanded={isOpen}
                            onClick={(e) => (e.stopPropagation(), toggle(r.id))}
                          >
                            <Icon name="forward" size={12} stroke={2.6} />
                          </button>
                          {when(r.started)}
                        </span>
                      </td>
                      <td>{nameOf(r.plan)}</td>
                      <td className="muted">{kindName[r.kind] ?? r.kind}</td>
                      <td className="muted">
                        {r.result !== "ok" && r.result !== "warning"
                          ? "—"
                          : r.kind === "backup" || r.kind === "full"
                            ? r.changed
                              ? `${r.changed.toLocaleString()} changed`
                              : "none changed"
                            : r.files
                              ? r.files.toLocaleString()
                              : "—"}
                      </td>
                      <td className="mono muted" style={{ textAlign: "right", fontSize: 11 }}>
                        {r.storedBytes ? bytes(r.storedBytes) : "—"}
                      </td>
                      <td className="mono muted" style={{ textAlign: "right", fontSize: 11 }}>
                        {r.result === "waiting" ? "—" : secs >= 1 ? duration(secs) : "<1 s"}
                      </td>
                      <td style={{ maxWidth: 420 }}>
                        <span className={`tag ${resultTone(r.result)}`} title={r.message}>
                          {resultLabel(r)}
                        </span>
                        {r.message && (
                          <div className="small muted" style={{ marginTop: 3 }}>
                            {r.message}
                          </div>
                        )}
                      </td>
                    </tr>
                    {isOpen && (
                      <tr>
                        <td colSpan={7} style={{ borderTop: 0, paddingTop: 0 }}>
                          <div
                            className="mono"
                            style={{
                              fontSize: 11,
                              lineHeight: 1.6,
                              background: "var(--sunk)",
                              borderRadius: 8,
                              padding: "10px 14px",
                              whiteSpace: "pre-wrap",
                              wordBreak: "break-word",
                              userSelect: "text",
                              WebkitUserSelect: "text",
                              maxHeight: 360,
                              overflow: "auto",
                            }}
                          >
                            {!logs[r.id]
                              ? "Loading…"
                              : logs[r.id].length
                                ? logs[r.id].join("\n")
                                : "No log for this run: it ran before Keepr kept logs."}
                          </div>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        )}
        <div ref={end} className="small faint" style={{ padding: "12px 20px", textAlign: "center" }}>
          {runs.length > 0 &&
            (failed
              ? `Couldn't read the history: ${failed}`
              : more
                ? "Loading more…"
                : `That's everything: ${runs.length.toLocaleString()} ${filter === "all" ? "runs" : filter === "problems" ? "problems" : "restores"}.`)}
        </div>
      </section>
    </div>
  );
}
