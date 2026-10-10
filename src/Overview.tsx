// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Overview: are my files kept? A headline, what needs attention, each plan with its last 30
// days, and the space the backups take.

import { useCallback, useEffect, useRef, useState } from "react";
import { api, type PlanSummary, type Run } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import { ago, bytes, count, longDate, next, when } from "./format";
import { RecoverySheet } from "./Plans";
import { DestIcon, jobTitle, PlanProgress, StopButton, useAct } from "./ui";
import { Connection, SpaceMeter } from "./DestStatus";
import { resultLabel, resultTone, TONE_COLOR } from "./runResult";

/** What a day's bar means, for its tooltip: "Thursday 24 September: 12 backups, 88 MB stored". */
function dayTip(d: PlanSummary["days"][number], daysAgo: number): string {
  const date = new Date();
  date.setDate(date.getDate() - daysAgo);
  const label =
    daysAgo === 0
      ? "Today"
      : daysAgo === 1
        ? "Yesterday"
        : date.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
  if (!d.ran) return `${label}: no backup`;
  const parts = [d.count ? count(d.count, "backup") : "no backup completed", d.count ? `${bytes(d.added)} sent` : ""].filter(Boolean);
  return `${label}: ${parts.join(", ")}${d.failed ? " · one didn't finish" : ""}`;
}

function Strata({ p, compact }: { p: PlanSummary; compact?: boolean }) {
  const max = Math.max(1, ...p.days.map((d) => d.added));
  return (
    <div className="strata" style={compact ? { height: 26, gap: 2, width: 220, flexShrink: 0 } : undefined}>
      {p.days.map((d, i) => {
        const h = d.failed ? 100 : d.ran ? 12 + 88 * Math.sqrt(d.added / max) : 5;
        const cls = d.failed ? "bad" : !d.ran ? "none" : i === p.days.length - 1 ? "hi" : "";
        // The bar's hover area is its full column, so thin days are easy to point at.
        return (
          <div
            key={i}
            title={dayTip(d, p.days.length - 1 - i)}
            style={{ flex: 1, height: "100%", display: "flex", alignItems: "flex-end", background: "transparent", minHeight: 0 }}
          >
            <div className={cls} style={{ height: `${h}%`, width: "100%", flex: "none" }} />
          </div>
        );
      })}
    </div>
  );
}

const TILE: Record<string, string> = { folder: "folder", photos: "image", notes: "file" };

function PlanCard({ p }: { p: PlanSummary }) {
  const { go, ov } = useApp();
  const act = useAct();
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
            <span className={`tag ${bad ? "bad" : "warn"}`}>
              {bad ? "Didn't finish" : p.status === "stale" ? "No recent backup" : "Waiting"}
            </span>
          </div>
          <span className="small muted ellipsis" title={p.message}>
            {p.message} · last backup {ago(p.lastSuccess)}
          </span>
        </div>
        <Strata p={p} compact />
        <button
          className="btn"
          onClick={() => go({ name: "plans", plan: p.id })}
          title="Change what this plan backs up, where, when and how"
        >
          Edit…
        </button>
        <button className="btn" onClick={() => act(() => api.backUp(p.id))}>
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
            <button
              onClick={() => go({ name: "plans", plan: p.id })}
              style={{ border: 0, background: "none", padding: 0, fontSize: 15, fontWeight: 600 }}
            >
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
        <button
          className="btn"
          onClick={() => go({ name: "plans", plan: p.id })}
          title="Change what this plan backs up, where, when and how"
        >
          Edit…
        </button>
        <button className="btn" disabled={p.snapshots === 0} onClick={() => go({ name: "restore", plan: p.id })}>
          Restore…
        </button>
        {running ? (
          <StopButton job={ov!.job!} iconOnly label={`Stop ${p.name}`} />
        ) : waiting ? (
          <button
            className="btn"
            style={{ width: 30, padding: 0 }}
            aria-label={`Cancel ${p.name}'s waiting backup`}
            title="Cancel"
            onClick={() => act(() => api.cancel(waiting.id))}
          >
            <Icon name="stop" size={12} />
          </button>
        ) : (
          <button
            className="btn"
            style={{ width: 30, padding: 0 }}
            aria-label={`Back up ${p.name} now`}
            title="Back up now"
            onClick={() => act(() => api.backUp(p.id))}
          >
            <Icon name="play" size={14} />
          </button>
        )}
      </div>
      {(running || waiting) && <PlanProgress job={running ? ov?.job : null} queued={waiting} />}
      <div className="col" style={{ gap: 5 }}>
        <Strata p={p} />
        <div className="row tiny faint" style={{ justifyContent: "space-between" }}>
          <span>30 days ago</span>
          <span>
            Data sent per day · {p.snapshots.toLocaleString()} snapshot{p.snapshots === 1 ? "" : "s"}
          </span>
          <span>Today</span>
        </div>
      </div>
      <div className="stats4">
        <div>
          <span>Last backup</span>
          <span>
            {p.status === "never"
              ? "Not yet"
              : running
                ? "Running now"
                : `${ago(p.lastSuccess)}${p.lastChanged ? ` · ${p.lastChanged.toLocaleString()} changed` : ""}`}
          </span>
        </div>
        <div title="What this plan's backup takes up at its destination now, all versions included">
          <span>Backup size now</span>
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

/** Dragging plans into a new order by their grip. The list reorders as the pointer passes each
 *  card's middle; the order is saved on release. Pointer events rather than HTML drag and drop,
 *  which the window's file-drop handling gets in the way of. */
function useReorder(ids: string[], save: (ids: string[]) => Promise<void>) {
  const [order, setOrder] = useState<string[] | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const refs = useRef(new Map<string, HTMLElement>());
  const shown = order ?? ids;
  // A saved order stays until the plans come back in it.
  useEffect(() => {
    if (!dragging && order && order.join() === ids.join()) setOrder(null);
  }, [ids, order, dragging]);
  const grip = (id: string) => ({
    onPointerDown: (e: React.PointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      setOrder(shown);
      setDragging(id);
    },
    onPointerMove: (e: React.PointerEvent) => {
      if (dragging !== id) return;
      const scroller = (e.currentTarget as HTMLElement).closest(".content");
      if (scroller) {
        const r = scroller.getBoundingClientRect();
        if (e.clientY < r.top + 40) scroller.scrollBy(0, -12);
        else if (e.clientY > r.bottom - 40) scroller.scrollBy(0, 12);
      }
      setOrder((cur) => {
        const list = (cur ?? shown).filter((x) => x !== id);
        let at = list.length;
        for (let i = 0; i < list.length; i++) {
          const el = refs.current.get(list[i]);
          if (!el) continue;
          const r = el.getBoundingClientRect();
          if (e.clientY < r.top + r.height / 2) {
            at = i;
            break;
          }
        }
        list.splice(at, 0, id);
        return cur && list.join() === cur.join() ? cur : list;
      });
    },
    onPointerUp: () => {
      if (dragging !== id) return;
      setDragging(null);
      if (order && order.join() !== ids.join()) save(order).catch(() => setOrder(null));
      else setOrder(null);
    },
    onPointerCancel: () => {
      setDragging(null);
      setOrder(null);
    },
  });
  const ref = (id: string) => (el: HTMLElement | null) => {
    if (el) refs.current.set(id, el);
    else refs.current.delete(id);
  };
  return { shown, dragging, grip, ref };
}

const ROW_HEIGHT = 18;
const ROW_GAP = 10;

export default function Overview() {
  const { ov, go, refresh } = useApp();
  const [recent, setRecent] = useState<Run[]>([]);
  const [recovery, setRecovery] = useState<PlanSummary | null>(null);
  // As many recent runs as fit in the space left under Destinations: the list takes no height of
  // its own (flex-basis 0), so the card is as tall as the column allows, and rows are counted in.
  // Watched from when the list appears, since it isn't there while the Welcome page shows.
  const [fits, setFits] = useState(5);
  const watch = useRef<ResizeObserver | null>(null);
  const listRef = useCallback((el: HTMLDivElement | null) => {
    watch.current?.disconnect();
    watch.current = null;
    if (!el) return;
    watch.current = new ResizeObserver(() => setFits(Math.max(3, Math.floor((el.clientHeight + ROW_GAP) / (ROW_HEIGHT + ROW_GAP)))));
    watch.current.observe(el);
  }, []);
  // Read again when the plans are (a run finished, or the half-minute refresh), not on every
  // progress tick of a running job, which rebuilds the overview many times a second.
  useEffect(() => {
    let live = true;
    api.history(Math.min(fits, 100)).then(
      (r) => live && setRecent(r),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [ov?.plans, fits]);
  const reorder = useReorder(ov?.plans.map((p) => p.id) ?? [], async (ids) => {
    await api.reorderPlans(ids);
    await refresh();
  });
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
  const headline = job
    ? jobTitle(job)
    : failed.length
      ? `${failed[0].name} needs attention`
      : kept.length === 0
        ? "Nothing is kept yet"
        : "Everything is kept";
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
            {latest
              ? `Last backup ${ago(latest.lastSuccess)} to ${latest.destination}.`
              : "The first backup copies everything, so it takes longest."}
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

        {reorder.shown.map((id) => {
          const p = ov.plans.find((x) => x.id === id);
          if (!p) return null;
          return (
            <div key={id} ref={reorder.ref(id)} className={`plan-slot${reorder.dragging === id ? " lifted" : ""}`}>
              {ov.plans.length > 1 && (
                <button
                  className="plan-grip"
                  aria-label={`Move ${p.name}`}
                  title="Drag to change the order of your plans"
                  {...reorder.grip(id)}
                >
                  <Icon name="grip" size={16} />
                </button>
              )}
              <PlanCard p={p} />
            </div>
          );
        })}
        <button
          className="card"
          onClick={() => go({ name: "plans", isNew: true })}
          title="Make another plan ⌘N"
          style={{
            height: 56,
            border: "1.5px dashed var(--line2)",
            background: "transparent",
            color: "var(--accent-text)",
            fontWeight: 600,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            flexShrink: 0,
          }}
        >
          <Icon name="plus" size={14} stroke={2.4} />
          Add a plan
        </button>
      </section>

      <aside className="col" style={{ width: 340, flexShrink: 0, gap: 16 }}>
        <div className="card pad col" style={{ gap: 12 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="caps">Space used</span>
            <a
              href="#"
              onClick={(e) => (e.preventDefault(), go({ name: "sizemap" }))}
              className="small"
              title="See which folders take the space"
            >
              Size map
            </a>
          </div>
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
              <DestIcon kind={d.kind} label={d.kindLabel} />
              <div className="grow col" style={{ gap: 5 }}>
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <span style={{ fontWeight: 600 }}>{d.name}</span>
                  <Connection d={d} lower />
                </div>
                <SpaceMeter d={d} height={5} />
                <span className="small faint">
                  {bytes(d.keeprBytes)} from Keepr{d.free != null ? ` · ${bytes(d.free)} free` : ""}
                </span>
              </div>
            </div>
          ))}
        </div>

        <div className="card pad col" style={{ gap: 10, flexGrow: 1, minHeight: 200 }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="caps">Recent</span>
            <a href="#" className="small" onClick={(e) => (e.preventDefault(), go({ name: "activity" }))}>
              All activity
            </a>
          </div>
          <div ref={listRef} className="col" style={{ flex: "1 1 0", minHeight: 0, overflow: "hidden", gap: ROW_GAP }}>
            {recent.length === 0 && <span className="small muted">Nothing yet.</span>}
            {recent.slice(0, fits).map((r) => (
              <div key={r.id} className="row small" style={{ gap: 10, alignItems: "center", height: ROW_HEIGHT, flexShrink: 0 }}>
                <span className="mono faint nowrap" style={{ width: 104, fontSize: 11, flexShrink: 0 }}>
                  {when(r.started).replace(/^Today /, "")}
                </span>
                <span className="grow ellipsis" title={r.message}>
                  {nameOf(r.plan)} · {r.message}
                </span>
                <span style={{ color: TONE_COLOR[resultTone(r.result)] }}>{resultLabel(r, true)}</span>
              </div>
            ))}
          </div>
        </div>
      </aside>
      {recovery && <RecoverySheet plan={recovery.id} name={recovery.name} onClose={() => setRecovery(null)} />}
    </div>
  );
}
