// Activity: what is running now, step by step, and the history of every backup, check, tidy-up
// and restore.

import { useEffect, useState } from "react";
import { api, type Run } from "./api";
import { useApp } from "./App";
import { Icon } from "./icons";
import { Seg } from "./ui";
import { bytes, duration, secondsLeft, when } from "./format";

const STEPS = ["Look", "Compare", "Pack", "Send", "Confirm"];

function stepOf(stage: string): number {
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
  const pct = job.bytesToRead > 0 ? Math.min(100, (100 * job.bytesRead) / job.bytesToRead) : job.stage === "Saving" ? 99 : 0;
  const step = stepOf(job.stage);
  const verb = job.kind === "restore" ? "Restoring from" : job.kind === "check" ? "Checking" : job.kind === "prune" ? "Tidying up" : "Keeping";
  const skipped = job.dupBytes;
  return (
    <>
      <div className="col" style={{ gap: 4 }}>
        <h1>
          {verb} {job.planName}
          <span style={{ color: "var(--accent)" }}>…</span>
        </h1>
        <span className="muted" style={{ fontSize: 14 }}>
          {job.kind === "full" ? "A full backup: every file is read again." : job.kind === "backup" ? "An incremental backup." : job.stage}
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
                    <div className="row" style={{ gap: 6, fontWeight: 600, color: done ? "var(--ink)" : now ? "var(--accent-text)" : "var(--ink3)" }}>
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
              <span className="muted">{job.paused ? "Paused" : secondsLeft(job.etaSecs)}</span>
              <button className="btn" onClick={() => api.pause(!job.paused)}>
                <Icon name={job.paused ? "play" : "pause"} size={13} stroke={2.4} />
                {job.paused ? "Resume" : "Pause"}
              </button>
              <button className="btn" onClick={() => api.cancel(job.id)}>
                Stop
              </button>
            </div>
            <div className="progress">
              <div className="soft" style={{ width: `${job.bytesToRead ? (100 * Math.min(skipped, job.bytesRead)) / job.bytesToRead : 0}%` }} />
              <div className="solid" style={{ width: `${job.bytesToRead ? (100 * Math.max(0, job.bytesRead - skipped)) / job.bytesToRead : 0}%` }} />
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
              {job.current.startsWith(home) ? `~${job.current.slice(home.length)}` : job.current}
            </div>
          )}
        </section>
        {isBackup && (
          <aside className="card" style={{ width: 320, flexShrink: 0, padding: "20px 22px", display: "flex", flexDirection: "column", gap: 14 }}>
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
  useEffect(() => {
    api.history(300).then(setRuns);
  }, [ov]);
  const nameOf = (id: string) => ov?.plans.find((p) => p.id === id)?.name ?? "A deleted plan";
  const shown = runs.filter((r) => (filter === "all" ? true : filter === "problems" ? r.result !== "ok" : r.kind === "restore"));
  const kindName: Record<string, string> = { backup: "Incremental", full: "Full re-read", check: "Check", prune: "Tidy up", restore: "Restore" };
  return (
    <div className="content col" style={{ gap: 18 }}>
      {job ? <Running /> : <h1>Activity</h1>}
      <section className="card" style={{ display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <div className="row" style={{ height: 46, padding: "0 20px", borderBottom: "1px solid var(--line)" }}>
          <h2 className="grow">History</h2>
          <Seg label="Filter" value={filter} onChange={setFilter} options={[["all", "All"], ["problems", "Problems"], ["restores", "Restores"]]} />
        </div>
        {shown.length === 0 ? (
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
              {shown.map((r) => {
                const secs = (new Date(r.finished).getTime() - new Date(r.started).getTime()) / 1000;
                const tone = r.result === "ok" ? "ok" : r.result === "failed" ? "bad" : r.result === "cancelled" ? "plain" : "warn";
                return (
                  <tr key={r.id}>
                    <td className="muted nowrap">{when(r.started)}</td>
                    <td>{nameOf(r.plan)}</td>
                    <td className="muted">{kindName[r.kind] ?? r.kind}</td>
                    <td className="muted">{r.kind === "backup" || r.kind === "full" ? (r.changed ? `${r.changed.toLocaleString()} changed` : "none changed") : r.files ? r.files.toLocaleString() : "—"}</td>
                    <td className="mono muted" style={{ textAlign: "right", fontSize: 11 }}>
                      {r.storedBytes ? bytes(r.storedBytes) : "—"}
                    </td>
                    <td className="mono muted" style={{ textAlign: "right", fontSize: 11 }}>
                      {secs > 0 ? duration(secs) : "—"}
                    </td>
                    <td style={{ maxWidth: 420 }}>
                      <span className={`tag ${tone}`} title={r.message}>
                        {r.result === "ok" ? (r.kind === "check" ? "All intact" : "Complete") : r.result === "cancelled" ? "Stopped" : r.result === "waiting" ? "Waiting" : r.result === "warning" ? "Done, with problems" : "Failed"}
                      </span>
                      {r.message && <div className="small muted" style={{ marginTop: 3 }}>{r.message}</div>}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
