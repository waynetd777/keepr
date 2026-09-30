// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Restore: pick a moment on the snapshot strip, tick files and folders as they were then, see
// any file's versions, and put them back where they were or into another folder.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, type Comparison, type Conflict, type Entry, type SnapInfo, type Version } from "./api";
import { statusDot, useApp } from "./context";
import { Icon } from "./icons";
import { ClearButton, PlanProgress, Sheet, Switch, useAct, useToast } from "./ui";
import { bytes, dayKey, dayLabel, longWhen, tilde, when } from "./format";

const DAYS_SHOWN = 10;

function iconFor(e: Entry): string {
  if (e.kind === "dir") return "folder";
  if (e.kind === "link") return "link";
  return /\.(jpe?g|png|heic|gif|tiff?|webp|raw|cr2|nef|dng)$/i.test(e.name) ? "image" : "file";
}

function Strip({ snaps, sel, setSel }: { snaps: SnapInfo[]; sel: number; setSel: (i: number) => void }) {
  // Days with snapshots, newest last; ten at a time, paged with the earlier button.
  const days = useMemo(() => {
    const out: { key: string; label: string; today: boolean; items: number[] }[] = [];
    snaps.forEach((s, i) => {
      const k = dayKey(s.time);
      const last = out[out.length - 1];
      if (last && last.key === k) last.items.push(i);
      else out.push({ key: k, ...(({ short, today }) => ({ label: short, today }))(dayLabel(s.time)), items: [i] });
    });
    return out;
  }, [snaps]);
  const selDay = days.findIndex((d) => d.items.includes(sel));
  const end = Math.min(days.length, Math.max(DAYS_SHOWN, selDay + 1 + Math.max(0, DAYS_SHOWN - 1 - (days.length - 1 - selDay))));
  const start = Math.max(0, Math.min(selDay, end - DAYS_SHOWN));
  const shown = days.slice(start, start + DAYS_SHOWN);
  const olderCount = days.slice(0, start).reduce((n, d) => n + d.items.length, 0);
  const max = Math.max(1, ...snaps.map((s) => s.addedBytes));
  return (
    <div className="snapstrip">
      <button
        disabled={olderCount === 0}
        title={olderCount ? `${olderCount} earlier snapshots` : "No earlier snapshots"}
        onClick={() => setSel(days[Math.max(0, start - 1)].items.slice(-1)[0])}
        style={{
          width: 88,
          flexShrink: 0,
          border: 0,
          background: "transparent",
          display: "flex",
          flexDirection: "column",
          alignItems: "flex-start",
          justifyContent: "flex-end",
          gap: 6,
          padding: 0,
          color: "var(--ink2)",
          fontSize: 11,
          textAlign: "left",
        }}
      >
        <span className="mono" style={{ color: "var(--ink)" }}>
          +{olderCount.toLocaleString()}
        </span>
        {snaps.length ? `back to ${new Date(snaps[0].time).toLocaleDateString(undefined, { day: "numeric", month: "short" })}` : ""}
      </button>
      {shown.map((d) => (
        <div key={d.key} className="snapday">
          <div className="bars">
            {d.items.map((i) => {
              const s = snaps[i];
              const h = 10 + 34 * Math.sqrt(s.addedBytes / max);
              return (
                <button
                  key={s.id}
                  className={`snapbar${i === sel ? " on" : ""}`}
                  aria-label={longWhen(s.time)}
                  title={`${longWhen(s.time)} · ${s.changed.toLocaleString()} changed`}
                  onClick={() => setSel(i)}
                >
                  <span style={{ height: h }} />
                </button>
              );
            })}
          </div>
          <span
            className="tiny"
            style={{ color: d.items.includes(sel) ? "var(--accent-text)" : "var(--ink3)", fontWeight: d.items.includes(sel) ? 600 : 400 }}
          >
            {d.label}
          </span>
        </div>
      ))}
    </div>
  );
}

function CompareSheet({ plan, snapshot, path, onClose }: { plan: string; snapshot: string; path: string; onClose: () => void }) {
  const [c, setC] = useState<Comparison | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    api.compare(plan, snapshot, path).then(setC, (e) => setErr(String(e)));
  }, [plan, snapshot, path]);
  const name = path.split("/").pop();
  return (
    <Sheet
      title={`Compare ${name}`}
      subtitle="The version from the backup against the file on this Mac now."
      width={820}
      onClose={onClose}
      foot={
        <button className="btn primary" onClick={onClose}>
          Done
        </button>
      }
    >
      <div className="sheet-body">
        {err && <span style={{ color: "var(--red)" }}>{err}</span>}
        {!c && !err && <span className="muted">Comparing…</span>}
        {c && !c.currentExists && <span>The file isn't on this Mac any more. Restoring it puts back the version from the backup.</span>}
        {c && c.currentExists && c.identical && <span>They're the same: the file hasn't changed since this version.</span>}
        {c && c.currentExists && !c.identical && !c.text && (
          <span>
            They differ ({bytes(c.backupSize)} in the backup, {bytes(c.currentSize)} now). Only text files can be compared line by line; use
            Quick Look to see the version from the backup.
          </span>
        )}
        {c && c.text && !c.identical && (
          <>
            <span className="small muted">
              <span style={{ color: "var(--red)" }}>{c.removed.toLocaleString()} lines only in the backup</span> ·{" "}
              <span style={{ color: "var(--accent-text)" }}>{c.added.toLocaleString()} lines only on this Mac</span>
            </span>
            <div className="diff">
              {c.lines.map((l, i) =>
                l.kind === "gap" ? (
                  <div key={i} className="gap">
                    ⋯
                  </div>
                ) : (
                  <div key={i} className={l.kind}>
                    <span className="n">{l.old ?? ""}</span>
                    <span className="n">{l.new ?? ""}</span>
                    <span>{(l.kind === "added" ? "+ " : l.kind === "removed" ? "− " : "  ") + l.text}</span>
                  </div>
                ),
              )}
            </div>
          </>
        )}
      </div>
    </Sheet>
  );
}

type SortBy = "name" | "modified" | "size" | "versions";

// A column heading that sorts the list: click to sort by it, again to reverse.
function SortHead({
  by,
  sort,
  setSort,
  className,
  style,
  children,
}: {
  by: SortBy;
  sort: { by: SortBy; up: boolean };
  setSort: (s: { by: SortBy; up: boolean }) => void;
  className?: string;
  style?: React.CSSProperties;
  children: React.ReactNode;
}) {
  const on = sort.by === by;
  return (
    <button
      className={`sorthead${on ? " on" : ""}${className ? ` ${className}` : ""}`}
      style={style}
      aria-sort={on ? (sort.up ? "ascending" : "descending") : "none"}
      onClick={() => setSort({ by, up: on ? !sort.up : true })}
    >
      {children}
      {on && <span aria-hidden="true">{sort.up ? "▲" : "▼"}</span>}
    </button>
  );
}

export default function Restore() {
  const { ov, screen, home, go } = useApp();
  const act = useAct();
  const toast = useToast();
  const plans = ov?.plans ?? [];
  const planId = (screen.name === "restore" && screen.plan) || plans.find((p) => p.snapshots > 0)?.id || plans[0]?.id;
  const plan = plans.find((p) => p.id === planId);
  const [snaps, setSnaps] = useState<SnapInfo[] | null>(null);
  const [err, setErr] = useState("");
  const [sel, setSel] = useState(0);
  const [kids, setKids] = useState<Record<string, Entry[]>>({});
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [checked, setChecked] = useState<Map<string, Entry>>(new Map());
  const [picked, setPicked] = useState<Entry | null>(null);
  const [versions, setVersions] = useState<Version[] | null>(null);
  const [ver, setVer] = useState(0);
  const [showDeleted, setShowDeleted] = useState(true);
  const [dest, setDest] = useState<"original" | "folder">("original");
  const [folder, setFolder] = useState("");
  const [conflict, setConflict] = useState<Conflict>("keepBoth");
  // Opened from Search: that snapshot, and that file found and picked.
  const focus = screen.name === "restore" ? screen.focus : undefined;
  const [query, setQuery] = useState(screen.name === "restore" ? (screen.query ?? focus?.path.split("/").pop() ?? "") : "");
  const [hits, setHits] = useState<Entry[] | null>(null);
  const [comparing, setComparing] = useState<{ snapshot: string; path: string } | null>(null);
  // Folders stay first, as in Finder; within them, by the column clicked, again to reverse.
  const [sort, setSort] = useState<{ by: SortBy; up: boolean }>({ by: "name", up: true });

  useEffect(() => {
    if (!planId) return;
    setSnaps(null);
    setErr("");
    api.snapshots(planId).then(
      (s) => {
        setSnaps(s);
        const at = focus ? s.findIndex((x) => x.id === focus.snapshot) : -1;
        setSel(at >= 0 ? at : s.length - 1);
      },
      (e) => setErr(String(e)),
    );
  }, [planId, focus]);

  // A job of this plan's has ended: a backup adds a snapshot and Remove from backup rewrites them,
  // so fetch them again, staying on the same moment (or the newest, if that's where we were).
  const planJob = ov?.job?.plan === planId ? ov.job.id : undefined;
  const lastJob = useRef(planJob);
  useEffect(() => {
    const ended = lastJob.current && lastJob.current !== planJob;
    lastJob.current = planJob;
    if (!ended || !planId || !snaps) return;
    const was = snaps[sel];
    const atNewest = sel === snaps.length - 1;
    api.snapshots(planId).then(
      (s) => {
        const at = atNewest ? -1 : s.findIndex((x) => x.time === was?.time);
        setSnaps(s);
        setSel(at >= 0 ? at : s.length - 1);
        setPicked(null);
      },
      () => {},
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planJob]);

  const snap = snaps?.[sel];
  const load = useCallback(
    async (path: string) => {
      if (!planId || !snap) return;
      const list = await api.listDir(planId, snap.id, path, showDeleted).catch((e) => {
        setErr(String(e));
        return [] as Entry[];
      });
      setKids((k) => ({ ...k, [path]: list }));
    },
    [planId, snap, showDeleted],
  );

  // A new moment or the deleted switch: reload the folders that are open.
  useEffect(() => {
    if (!snap) return;
    setKids({});
    load("");
    open.forEach((p) => load(p));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snap?.id, showDeleted]);

  // The first time: open each source, so there's something to see.
  useEffect(() => {
    const roots = kids[""];
    if (roots && open.size === 0 && roots.length <= 3) {
      const s = new Set(roots.filter((r) => r.kind === "dir").map((r) => r.path));
      setOpen(s);
      s.forEach((p) => load(p));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kids[""]]);

  useEffect(() => {
    if (!planId || !snap || !query.trim()) {
      setHits(null);
      return;
    }
    const t = window.setTimeout(
      () =>
        api.search(planId, snap.id, query.trim()).then(
          (h) => {
            setHits(h);
            const f = focus && h.find((e) => e.path === focus.path);
            if (f) setPicked(f);
          },
          (e) => setErr(String(e)),
        ),
      250,
    );
    return () => window.clearTimeout(t);
  }, [query, planId, snap, focus]);

  const pickedFile = picked?.kind === "file" ? picked.path : undefined;
  useEffect(() => {
    setVersions(null);
    setVer(0);
    if (pickedFile && planId) api.versions(planId, pickedFile).then(setVersions);
  }, [pickedFile, planId]);

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input, select, textarea") || !snaps) return;
      if (e.key === "ArrowLeft") setSel((s) => Math.max(0, s - 1));
      if (e.key === "ArrowRight") setSel((s) => Math.min(snaps.length - 1, s + 1));
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [snaps]);

  if (!plan) {
    return (
      <div className="content">
        <div className="empty">
          <h1>Nothing to restore yet</h1>
          <span>Once a plan has backed up, its files and their versions are here.</span>
          <button className="btn primary" onClick={() => go({ name: "plans", isNew: true })}>
            Make a plan
          </button>
        </div>
      </div>
    );
  }

  const toggleOpen = (p: string) => {
    const s = new Set(open);
    if (s.has(p)) s.delete(p);
    else {
      s.add(p);
      if (!kids[p]) load(p);
    }
    setOpen(s);
  };
  const toggleCheck = (e: Entry) => {
    const m = new Map(checked);
    if (m.has(e.path)) m.delete(e.path);
    else m.set(e.path, e);
    setChecked(m);
  };

  // Keyed by place in the tree as well as path: a backup made while one source was inside another
  // holds the same path twice.
  const rows: { e: Entry; depth: number; key: string }[] = [];
  const sorted = (list: Entry[]) => {
    const dir = sort.up ? 1 : -1;
    const value = (e: Entry) => (sort.by === "modified" ? e.mtime : sort.by === "size" ? e.size : sort.by === "versions" ? e.versions : 0);
    return [...list].sort(
      (a, b) =>
        Number(a.kind !== "dir") - Number(b.kind !== "dir") ||
        dir * (value(a) - value(b)) ||
        dir * a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }),
    );
  };
  const walk = (path: string, depth: number, under: string) => {
    sorted(kids[path] ?? []).forEach((e, i) => {
      // By position: a source's name is its whole path, so names joined up can collide.
      const key = `${under}.${i}`;
      rows.push({ e, depth, key });
      if (e.kind === "dir" && open.has(e.path) && e.tag !== "deleted") walk(e.path, depth + 1, key);
    });
  };
  if (hits) sorted(hits).forEach((e, i) => rows.push({ e, depth: 0, key: `${i}` }));
  else walk("", 0, "");

  const items = [...checked.values()];
  const total = items.reduce((n, e) => n + e.size, 0);
  const start = async (snapshot: string, paths: string[]) => {
    if (dest === "folder" && !folder) {
      toast("Choose a folder to restore into.");
      return;
    }
    const target = dest === "folder" ? { kind: "folder" as const, path: folder } : { kind: "original" as const };
    await act(() => api.restore(plan.id, snapshot, paths, target, conflict));
    // Back in place, a restored file is on the Mac again; a backup after it puts that in a
    // snapshot, so it stops showing as deleted.
    if (dest === "original") await api.backUp(plan.id).catch(() => {});
    toast(`Restoring ${paths.length === 1 ? paths[0].split("/").pop() : `${paths.length} items`}. Activity shows how it's going.`);
  };
  const chooseFolder = async () => {
    const [f] = await api.chooseFolders("Restore into this folder", false, folder || undefined);
    if (f) {
      setFolder(f);
      setDest("folder");
    }
  };
  const version = versions?.[ver];
  // Deleting a file or folder from every snapshot: asked twice, as it can't be undone.
  const removeFromBackup = async (e: Entry) => {
    const { ask } = await import("@tauri-apps/plugin-dialog");
    const where = tilde(e.path, home);
    const first = await ask(
      `Remove ${where} from every snapshot of ${plan.name}? Its space is freed, and none of its versions can be restored afterwards.`,
      { title: `Remove ${e.name} from the backup`, kind: "warning", okLabel: "Continue", cancelLabel: "Cancel" },
    );
    if (!first) return;
    const sure = await ask(`Delete every backed-up version of ${where}? This can't be undone.`, {
      title: "Are you sure?",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (!sure) return;
    await act(() => api.removePathData(plan.id, e.path));
    toast(`Removing ${e.name} from the backup. Activity shows how it's going.`);
  };
  // A restore from this plan running or waiting: the footer shows its progress instead.
  const restoring = ov?.job?.kind === "restore" && ov.job.plan === plan.id ? ov.job : null;
  const restoreQueued = ov?.queued.find((q) => q.plan === plan.id && q.kind === "restore");

  return (
    <>
      <div className="content col" style={{ gap: 16, padding: "20px 32px 18px", overflow: "hidden" }}>
        <div className="row" style={{ gap: 12 }}>
          <div className="tabs" role="tablist" aria-label="Plan to restore from">
            {plans.map((p) => (
              <button
                key={p.id}
                role="tab"
                aria-selected={p.id === plan.id}
                className={p.id === plan.id ? "on" : ""}
                onClick={() => go({ name: "restore", plan: p.id })}
                title={`${p.snapshots.toLocaleString()} snapshots`}
              >
                <span className={statusDot(p.status)} />
                {p.name}
                <span className="tiny faint">{p.snapshots.toLocaleString()}</span>
              </button>
            ))}
          </div>
          <label className="search" style={{ width: 360 }}>
            <Icon name="search" size={15} stroke={2} />
            <input
              aria-label="Find in this backup"
              placeholder={`Find in ${plan.name}`}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            {query && <ClearButton onClick={() => setQuery("")} />}
          </label>
          <span className="grow" />
          <label className="row small muted" style={{ gap: 8 }}>
            Show deleted files
            <Switch label="Show deleted files" on={showDeleted} onChange={setShowDeleted} />
          </label>
        </div>

        {err && (
          <div className="banner bad">
            <Icon name="warning" size={18} />
            <span className="text">{err}</span>
          </div>
        )}
        {snaps && snaps.length === 0 && !err && <div className="empty">{plan.name} hasn't finished a backup yet.</div>}

        {snap && (
          <section className="card" style={{ padding: "16px 20px 12px", display: "flex", flexDirection: "column", gap: 12, flexShrink: 0 }}>
            <div className="row" style={{ alignItems: "flex-end", gap: 16 }}>
              <div className="grow col" style={{ gap: 2 }}>
                <span className="small muted">Your files as they were on</span>
                <span style={{ fontFamily: "var(--display)", fontWeight: 650, letterSpacing: "-0.02em", fontSize: 26, lineHeight: 1.1 }}>
                  {longWhen(snap.time)}
                </span>
              </div>
              <span className="small muted" style={{ paddingBottom: 6 }}>
                Snapshot {(sel + 1).toLocaleString()} of {snaps!.length.toLocaleString()} · {snap.kind} · {snap.changed.toLocaleString()}{" "}
                changed
              </span>
              <div className="row" style={{ gap: 6 }}>
                <button className="btn" title="Earlier snapshot ←" disabled={sel === 0} onClick={() => setSel(sel - 1)}>
                  <Icon name="back" size={14} stroke={2} />
                  Earlier
                </button>
                <button className="btn" title="Later snapshot →" disabled={sel === snaps!.length - 1} onClick={() => setSel(sel + 1)}>
                  Later
                  <Icon name="forward" size={14} stroke={2} />
                </button>
                <button
                  className="btn"
                  title="Newest snapshot"
                  disabled={sel === snaps!.length - 1}
                  onClick={() => setSel(snaps!.length - 1)}
                >
                  Latest
                </button>
              </div>
            </div>
            <Strip snaps={snaps!} sel={sel} setSel={setSel} />
          </section>
        )}

        {snap && (
          <section className="card" style={{ flexGrow: 1, minHeight: 0, display: "flex", overflow: "hidden" }}>
            <div className="grow" style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
              <div className="files-head caps" style={{ letterSpacing: "0.04em" }}>
                <span style={{ width: 14 }} />
                <SortHead by="name" sort={sort} setSort={setSort} className="grow" style={{ paddingLeft: 40 }}>
                  {hits ? `${hits.length} found` : "Name"}
                </SortHead>
                <SortHead by="modified" sort={sort} setSort={setSort} style={{ width: 140 }}>
                  Modified
                </SortHead>
                <SortHead by="size" sort={sort} setSort={setSort} style={{ width: 76, justifyContent: "flex-end" }}>
                  Size
                </SortHead>
                <SortHead by="versions" sort={sort} setSort={setSort} style={{ width: 70, justifyContent: "flex-end" }}>
                  Versions
                </SortHead>
                <span style={{ width: 76 }} />
              </div>
              <div style={{ flexGrow: 1, overflow: "auto" }}>
                {rows.map(({ e, depth, key }) => {
                  const name = e.name.startsWith("/") ? tilde(e.name, home) : hits ? tilde(e.path, home) : e.name;
                  return (
                    <div key={key} className={`file-row${picked?.path === e.path ? " sel" : ""}${e.tag === "deleted" ? " gone" : ""}`}>
                      <input
                        type="checkbox"
                        aria-label={`Choose ${e.name}`}
                        checked={checked.has(e.path)}
                        onChange={() => toggleCheck(e)}
                      />
                      <span style={{ width: depth * 20, flexShrink: 0 }} />
                      {e.kind === "dir" && !hits && e.tag !== "deleted" ? (
                        <button
                          className={`disc${open.has(e.path) ? " open" : ""}`}
                          aria-label={`${open.has(e.path) ? "Collapse" : "Expand"} ${e.name}`}
                          onClick={() => toggleOpen(e.path)}
                        >
                          <Icon name="forward" size={12} stroke={2.6} />
                        </button>
                      ) : (
                        <span style={{ width: 16, flexShrink: 0 }} />
                      )}
                      <button className="name" onClick={() => setPicked(e)} onDoubleClick={() => e.kind === "dir" && toggleOpen(e.path)}>
                        <span className="grow row" style={{ gap: 8 }}>
                          <Icon
                            name={iconFor(e)}
                            style={{ color: e.kind === "dir" ? "var(--accent)" : "var(--ink2)", flexShrink: 0 }}
                            fill={e.kind === "dir" ? "currentColor" : "none"}
                            fillOpacity={e.kind === "dir" ? 0.22 : 0}
                          />
                          <span
                            className="ellipsis"
                            style={{
                              textDecoration: e.tag === "deleted" ? "line-through" : "none",
                              fontWeight: picked?.path === e.path ? 600 : 400,
                            }}
                          >
                            {name}
                          </span>
                          {e.kind === "dir" && e.items > 0 && <span className="tiny faint nowrap">{e.items.toLocaleString()} items</span>}
                        </span>
                        <span className="muted nowrap" style={{ width: 140 }}>
                          {e.kind === "dir" ? "" : when(new Date(e.mtime).toISOString())}
                        </span>
                        <span className="mono muted" style={{ width: 76, textAlign: "right", fontSize: 11 }}>
                          {bytes(e.size)}
                        </span>
                        <span className="mono muted" style={{ width: 70, textAlign: "right", fontSize: 11 }}>
                          {e.kind === "file" && e.versions ? e.versions : ""}
                        </span>
                        <span style={{ width: 76, display: "flex", justifyContent: "flex-end" }}>
                          {e.tag && (
                            <span className={`tag ${e.tag}`}>{e.tag === "new" ? "New" : e.tag === "changed" ? "Changed" : "Deleted"}</span>
                          )}
                        </span>
                      </button>
                    </div>
                  );
                })}
                {rows.length === 0 && <div className="empty">{hits ? "Nothing by that name in this snapshot." : "Loading…"}</div>}
              </div>
            </div>

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
                      <button key={v.snapshot} className={`version${i === ver ? " on" : ""}`} onClick={() => setVer(i)}>
                        <span className="dot" />
                        <span className="grow col" style={{ gap: 1 }}>
                          <span style={{ fontWeight: 600 }}>{when(v.time)}</span>
                          <span className="tiny muted">
                            {v.snapshot === snap.id
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
                      title="Quick Look (Space)"
                      disabled={!version}
                      onClick={() => version && act(() => api.quickLook(plan.id, version.snapshot, picked.path))}
                    >
                      <Icon name="eye" size={14} stroke={1.9} />
                      Quick Look
                    </button>
                    <button
                      className="btn"
                      title="Show what changed from the file on your Mac"
                      disabled={!version}
                      onClick={() => version && setComparing({ snapshot: version.snapshot, path: picked.path })}
                    >
                      Compare with current
                    </button>
                    <button className="btn" disabled={!version} onClick={() => version && start(version.snapshot, [picked.path])}>
                      Restore this version
                    </button>
                  </div>
                </>
              )}
              {picked && (
                <div style={{ padding: "0 16px 12px", display: picked.kind === "dir" ? "none" : "flex" }}>
                  <button
                    className="btn small danger"
                    onClick={() => removeFromBackup(picked)}
                    title="Take this out of every snapshot and free its space"
                  >
                    <Icon name="trash" size={13} />
                    Remove from backup…
                  </button>
                </div>
              )}
              {picked?.kind === "dir" && (
                <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 10, color: "var(--ink2)", lineHeight: 1.5 }}>
                  <span>
                    {picked.name.startsWith("/") ? tilde(picked.name, home) : picked.name} held {bytes(picked.size)} at this snapshot
                    {picked.items ? `, ${picked.items.toLocaleString()} items at its top` : ""}.
                  </span>
                  <span>Tick the folder to restore everything in it as it was then. Pick a file to see its versions.</span>
                  <button
                    className="btn small danger"
                    style={{ alignSelf: "flex-start" }}
                    onClick={() => removeFromBackup(picked)}
                    title="Take this folder out of every snapshot and free its space"
                  >
                    <Icon name="trash" size={13} />
                    Remove from backup…
                  </button>
                </div>
              )}
            </aside>
          </section>
        )}
      </div>

      {snap && (restoring || restoreQueued) && (
        <footer
          style={{
            height: 72,
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            gap: 18,
            padding: "0 32px",
            background: "var(--surface)",
            borderTop: "1px solid var(--line)",
          }}
        >
          <div className="col" style={{ gap: 2, minWidth: 150 }}>
            <span style={{ fontWeight: 600 }}>{restoring ? "Restoring" : "Restore waiting"}</span>
            <span className="small muted">
              {restoring
                ? `${restoring.filesRead.toLocaleString()} of ${restoring.filesToRead.toLocaleString()} files · ${bytes(restoring.bytesRead)} of ${bytes(restoring.bytesToRead)}`
                : "Starts when the current job finishes"}
            </span>
          </div>
          <div className="grow">
            <PlanProgress job={restoring} queued={restoring ? undefined : restoreQueued} />
          </div>
        </footer>
      )}
      {snap && !restoring && !restoreQueued && (
        <footer
          style={{
            height: 72,
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            gap: 18,
            padding: "0 32px",
            background: "var(--surface)",
            borderTop: "1px solid var(--line)",
          }}
        >
          <div className="col" style={{ gap: 2, minWidth: 150 }}>
            <span style={{ fontWeight: 600 }}>
              {items.length === 0 ? "Nothing chosen" : `${items.length} item${items.length === 1 ? "" : "s"} chosen`}
            </span>
            <span className="small muted">
              {bytes(total)} · from {when(snap.time)}
            </span>
          </div>
          <div style={{ width: 1, height: 36, background: "var(--line)" }} />
          <div className="col" style={{ gap: 4 }}>
            <span className="tiny faint">Restore to</span>
            <div className="seg" role="group" aria-label="Restore to">
              <button className={dest === "original" ? "on" : ""} aria-pressed={dest === "original"} onClick={() => setDest("original")}>
                Original location
              </button>
              <button
                className={dest === "folder" ? "on" : ""}
                aria-pressed={dest === "folder"}
                onClick={() => (folder ? setDest("folder") : chooseFolder())}
              >
                Another folder…
              </button>
            </div>
          </div>
          {dest === "folder" && folder && (
            <div className="col" style={{ gap: 4, minWidth: 0 }}>
              <span className="tiny faint">Folder</span>
              <button className="btn" onClick={chooseFolder} style={{ maxWidth: 260 }}>
                <Icon name="folder" size={14} />
                <span className="ellipsis">{tilde(folder, home)}</span>
              </button>
            </div>
          )}
          <label className="col" style={{ gap: 4 }}>
            <span className="tiny faint">{dest === "original" ? "If a file is already there" : "If the folder has one"}</span>
            <select className="input" value={conflict} onChange={(e) => setConflict(e.target.value as Conflict)}>
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
          <button
            className="btn primary big"
            disabled={items.length === 0}
            onClick={async () => {
              // A deleted item's last version is in the snapshot before this one.
              const here = items.filter((e) => e.tag !== "deleted").map((e) => e.path);
              const gone = items.filter((e) => e.tag === "deleted").map((e) => e.path);
              if (here.length) await start(snap.id, here);
              if (gone.length && sel > 0) await start(snaps![sel - 1].id, gone);
            }}
          >
            <Icon name="restore" size={15} stroke={2.2} />
            {items.length === 0 ? "Restore" : `Restore ${items.length} item${items.length === 1 ? "" : "s"}`}
          </button>
        </footer>
      )}
      {comparing && <CompareSheet plan={plan.id} snapshot={comparing.snapshot} path={comparing.path} onClose={() => setComparing(null)} />}
    </>
  );
}
