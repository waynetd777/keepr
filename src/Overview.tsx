// Overview: are my files kept? A headline, what needs attention, each plan with its last 30
// days, and the space the backups take.

import { useEffect, useState } from "react";
import { api, type PlanSummary, type Run } from "./api";
import { useApp } from "./App";
import { Icon } from "./icons";
import { ago, bytes, longDate, next, when } from "./format";
import { RecoverySheet } from "./Plans";
import { PlanProgress } from "./ui";

function Strata({ p, compact }: { p: PlanSummary; compact?: boolean }) {
  const max = Math.max(1, ...p.days.map((d) => d.added));
  return (
    <div className="strata" style={compact ? { height: 26, gap: 2, width: 220, flexShrink: 0 } : undefined} aria-hidden="true">
      {p.days.map((d, i) => {
        const h = d.failed ? 100 : d.ran ? 12 + 88 * Math.sqrt(d.added / max) : 5;
        const cls = d.failed ? "bad" : !d.ran ? "none" : i === p.days.length - 1 ? "hi" : "";
        return <div key={i} className={cls} style={{ height: `${h}%` }} />;
      })}
    </div>
  );
}

const TILE: Record<string, string> = { folder: "folder", photos: "image", notes: "file" };

function PlanCard({ p }: { p: PlanSummary }) {
  const { go, ov } = useApp();
  const running = ov?.job?.plan === p.id;
  const waiting = ov?.queued.find((q) => q.plan === p.id);
  if (p.status === "waiting" || p.status === "stale" || p.status === "failed") {
    const bad = p.status === "failed";
    return (
      <article className="card" style={{ padding: "16px 20px", display: "flex", alignItems: "center", gap: 14 }}>
        <div className={`tile ${bad ? "red" : "amber"}`}>
          <Icon name={TILE[p.icon]} size={20} />
        </div>
        <div className="grow col" style={{ gap: 3 }}>
          <div className="row" style={{ gap: 8 }}>
            <span style={{ fontSize: 15, fontWeight: 600 }}>{p.name}</span>
            <span className={`tag ${bad ? "bad" : "warn"}`}>{bad ? "Didn't finish" : p.status === "stale" ? "No recent backup" : "Waiting"}</span>
          </div>
          <span className="small muted ellipsis" title={p.message}>
            {p.message} · last backup {ago(p.lastSuccess)}
          </span>
        </div>
        <Strata p={p} compact />
        <button className="btn" onClick={() => api.backUp(p.id)}>
          Try again
        </button>
        <button className="btn" onClick={() => go({ name: "restore", plan: p.id })}>
          Restore…
        </button>
      </article>
    );
  }
  return (
    <article className="card plan-card">
      <div className="row" style={{ gap: 14 }}>
        <div className="tile">
          <Icon name={TILE[p.icon]} size={20} />
        </div>
        <div className="grow col" style={{ gap: 3 }}>
          <div className="row" style={{ gap: 8 }}>
            <button onClick={() => go({ name: "plans", plan: p.id })} style={{ border: 0, background: "none", padding: 0, fontSize: 15, fontWeight: 600 }}>
              {p.name}
            </button>
            {p.encrypted && (
              <span className="chip">
                <Icon name="lock" size={11} stroke={2.2} />
                Encrypted
              </span>
            )}
            <span className="chip">{p.schedule}</span>
            {!p.enabled && <span className="chip">Off</span>}
          </div>
          <span className="small muted ellipsis">
            {p.sources} → {p.destination}
          </span>
        </div>
        <button className="btn" disabled={p.snapshots === 0} onClick={() => go({ name: "restore", plan: p.id })}>
          Restore…
        </button>
        {running || waiting ? (
          <button className="btn" style={{ width: 30, padding: 0 }} aria-label={`Stop ${p.name}`} title="Stop" onClick={() => api.cancel(running ? ov!.job!.id : waiting!.id)}>
            <Icon name="stop" size={12} />
          </button>
        ) : (
          <button className="btn" style={{ width: 30, padding: 0 }} aria-label={`Back up ${p.name} now`} title="Back up now" onClick={() => api.backUp(p.id)}>
            <Icon name="play" size={14} />
          </button>
        )}
      </div>
      {(running || waiting) && <PlanProgress job={running ? ov?.job : null} queued={waiting} />}
      <div className="col" style={{ gap: 5 }}>
        <Strata p={p} />
        <div className="row tiny faint" style={{ justifyContent: "space-between" }}>
          <span>30 days ago</span>
          <span>Data stored per day · {p.snapshots.toLocaleString()} snapshot{p.snapshots === 1 ? "" : "s"}</span>
          <span>Today</span>
        </div>
      </div>
      <div className="stats4">
        <div>
          <span>Last backup</span>
          <span>{p.status === "never" ? "Not yet" : running ? "Running now" : `${ago(p.lastSuccess)}${p.lastChanged ? ` · ${p.lastChanged.toLocaleString()} changed` : ""}`}</span>
        </div>
        <div>
          <span>Stored</span>
          <span className="mono">{bytes(p.repoBytes)}</span>
        </div>
        <div>
          <span>Oldest version</span>
          <span>{longDate(p.oldest)}</span>
        </div>
        <div>
          <span>Next</span>
          <span>{p.enabled ? next(p.nextRun) : "Turned off"}</span>
        </div>
      </div>
    </article>
  );
}

function Welcome() {
  const { ov, go } = useApp();
  const hasDest = (ov?.destinations.length ?? 0) > 0;
  return (
    <div className="content">
      <div className="col" style={{ gap: 8, maxWidth: 620 }}>
        <h1>
          Welcome to Keepr<span style={{ color: "var(--accent)" }}>.</span>
        </h1>
        <p className="muted" style={{ fontSize: 14 }}>
          Two steps, and your files are kept: choose where backups go, then what to back up and how often.
        </p>
        <div className="col" style={{ gap: 12, marginTop: 18 }}>
          <div className="card pad row" style={{ gap: 14 }}>
            <div className="tile">{hasDest ? <Icon name="check" size={20} stroke={2.4} /> : <Icon name="server" size={20} />}</div>
            <div className="grow col" style={{ gap: 2 }}>
              <span style={{ fontWeight: 600 }}>1. Where to keep backups</span>
              <span className="small muted">A NAS or another computer on your network (SMB), or a folder or drive.</span>
            </div>
            <button className={`btn${hasDest ? "" : " primary"}`} onClick={() => go({ name: "destinations", add: true })}>
              {hasDest ? "Add another" : "Add a destination"}
            </button>
          </div>
          <div className="card pad row" style={{ gap: 14, opacity: hasDest ? 1 : 0.55 }}>
            <div className="tile">
              <Icon name="plans" size={20} />
            </div>
            <div className="grow col" style={{ gap: 2 }}>
              <span style={{ fontWeight: 600 }}>2. What to back up</span>
              <span className="small muted">Folders, how often, how long to keep versions, and whether to encrypt.</span>
            </div>
            <button className="btn primary" disabled={!hasDest} onClick={() => go({ name: "plans", isNew: true })}>
              Make a plan
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default function Overview() {
  const { ov, go } = useApp();
  const [recent, setRecent] = useState<Run[]>([]);
  const [recovery, setRecovery] = useState<PlanSummary | null>(null);
  useEffect(() => {
    api.history(5).then(setRecent);
  }, [ov]);
  if (!ov) return null;
  if (ov.plans.length === 0) return <Welcome />;

  const job = ov.job;
  const failed = ov.plans.filter((p) => p.status === "failed");
  const waiting = ov.plans.filter((p) => p.status === "waiting" || p.status === "stale");
  const kept = ov.plans.filter((p) => p.lastSuccess);
  const latest = kept.sort((a, b) => (b.lastSuccess ?? "").localeCompare(a.lastSuccess ?? ""))[0];
  const nextOne = ov.plans
    .filter((p) => p.enabled && p.nextRun && (p.status === "ok" || p.status === "never"))
    .map((p) => p.nextRun as string)
    .sort()[0];
  const headline = job ? `Backing up ${job.planName}` : failed.length ? `${failed[0].name} needs attention` : kept.length === 0 ? "Nothing is kept yet" : "Everything is kept";
  const nameOf = (id: string) => ov.plans.find((p) => p.id === id)?.name ?? "";

  return (
    <div className="content" style={{ display: "flex", gap: 28 }}>
      <section className="grow col" style={{ gap: 16 }}>
        <div className="col" style={{ gap: 6 }}>
          <h1>
            {headline}
            <span style={{ color: "var(--accent)" }}>{job ? "…" : "."}</span>
          </h1>
          <p className="muted" style={{ fontSize: 14 }}>
            {latest ? `Last backup ${ago(latest.lastSuccess)} to ${latest.destination}.` : "The first backup copies everything, so it takes longest."}
            {nextOne && ` The next one starts ${next(nextOne) === "now" ? "now" : `at ${next(nextOne)}`}.`}
          </p>
        </div>

        {ov.plans
          .filter((p) => p.recoveryUnsaved)
          .map((p) => (
            <div key={p.id} className="banner good" role="status">
              <Icon name="key" size={18} stroke={1.9} />
              <span className="text">
                <b>Save {p.name}'s recovery key.</b> Without it and the password, nobody can restore this encrypted backup.
              </span>
              <button className="btn small" onClick={() => setRecovery(p)}>
                Show recovery key
              </button>
            </div>
          ))}
        {waiting.map((p) => (
          <div key={p.id} className="banner" role="status">
            <Icon name="warning" size={18} stroke={1.9} />
            <span className="text">
              <b>{p.message}</b> {p.name} will catch up as soon as it can.
            </span>
            <button className="btn small" onClick={() => go({ name: "plans", plan: p.id })}>
              Show plan
            </button>
          </div>
        ))}

        {ov.plans.map((p) => (
          <PlanCard key={p.id} p={p} />
        ))}
        <button className="card" onClick={() => go({ name: "plans", isNew: true })} title="Make another plan ⌘N" style={{ height: 56, border: "1.5px dashed var(--line2)", background: "transparent", color: "var(--accent-text)", fontWeight: 600, display: "flex", alignItems: "center", justifyContent: "center", gap: 6, flexShrink: 0 }}>
          <Icon name="plus" size={14} stroke={2.4} />
          Add a plan
        </button>
      </section>

      <aside className="col" style={{ width: 340, flexShrink: 0, gap: 16 }}>
        <div className="card pad col" style={{ gap: 12 }}>
          <span className="caps">Space used</span>
          <div className="row" style={{ alignItems: "baseline", gap: 8 }}>
            <span className="big-number">{bytes(ov.storedBytes)}</span>
            <span className="muted">holds {bytes(ov.versionsBytes)} of versions</span>
          </div>
          <div className="progress">
            <div className="solid" style={{ width: `${ov.versionsBytes ? Math.max(2, (100 * ov.storedBytes) / ov.versionsBytes) : 0}%` }} />
          </div>
          <div className="col small muted" style={{ gap: 6 }}>
            <div className="row" style={{ gap: 8 }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: "var(--accent)" }} />
              <span className="grow">Stored after de-duplication and compression</span>
              <span className="mono nowrap">{bytes(ov.storedBytes)}</span>
            </div>
            <div className="row" style={{ gap: 8 }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: "var(--sunk)", border: "1px solid var(--line2)" }} />
              <span className="grow">Unchanged data not stored twice</span>
              <span className="mono nowrap">{bytes(Math.max(0, ov.versionsBytes - ov.storedBytes))}</span>
            </div>
          </div>
        </div>

        <div className="card pad col" style={{ gap: 14 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="caps">Destinations</span>
            <a href="#" onClick={(e) => (e.preventDefault(), go({ name: "destinations" }))} className="small">
              Manage
            </a>
          </div>
          {ov.destinations.map((d) => (
            <div key={d.id} className="row" style={{ gap: 12 }}>
              <Icon name={d.kind === "smb" ? "server" : "drive"} size={20} />
              <div className="grow col" style={{ gap: 5 }}>
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <span style={{ fontWeight: 600 }}>{d.name}</span>
                  <span className="small" style={{ color: d.connection === "missing" ? "var(--amber)" : "var(--ink2)" }}>
                    {d.kind === "smb" ? "SMB · " : ""}
                    {d.connection === "missing" ? "Not connected" : d.connection === "on demand" ? "connects when needed" : "connected"}
                  </span>
                </div>
                {d.total != null && (
                  <div className="meter" style={{ height: 5 }}>
                    <div style={{ width: `${Math.round((1 - (d.free ?? 0) / d.total) * 100)}%` }} />
                  </div>
                )}
                <span className="small faint">
                  {bytes(d.keeprBytes)} from Keepr{d.free != null ? ` · ${bytes(d.free)} free` : ""}
                </span>
              </div>
            </div>
          ))}
        </div>

        <div className="card pad col" style={{ gap: 10, flexGrow: 1 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="caps">Recent</span>
            <a href="#" className="small" onClick={(e) => (e.preventDefault(), go({ name: "activity" }))}>
              All activity
            </a>
          </div>
          {recent.length === 0 && <span className="small muted">Nothing yet.</span>}
          {recent.map((r) => (
            <div key={r.id} className="row small" style={{ gap: 10, alignItems: "flex-start" }}>
              <span className="mono faint" style={{ width: 70, fontSize: 11, flexShrink: 0 }}>
                {when(r.started).replace(/^Today /, "")}
              </span>
              <span className="grow ellipsis" title={r.message}>
                {nameOf(r.plan)} · {r.message}
              </span>
              <span style={{ color: r.result === "ok" ? "var(--accent-text)" : r.result === "failed" ? "var(--red)" : r.result === "cancelled" ? "var(--ink3)" : "var(--amber)" }}>
                {r.result === "ok" ? "✓" : r.result === "failed" ? "Failed" : r.result === "cancelled" ? "Stopped" : r.result === "waiting" ? "Waiting" : "!"}
              </span>
            </div>
          ))}
        </div>
      </aside>
      {recovery && <RecoverySheet plan={recovery.id} name={recovery.name} onClose={() => setRecovery(null)} />}
    </div>
  );
}
