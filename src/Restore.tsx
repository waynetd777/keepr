// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Restore: pick a moment on the snapshot strip, tick files and folders as they were then, see
// any file's versions, and put them back where they were or into another folder.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, type Comparison, type Conflict, type Entry, type SnapInfo, type Version } from "./api";
import { FileList, onlyOnMac, RowMenu, type Sort } from "./RestoreFiles";
import { RestoreFooter } from "./RestoreFooter";
import { VersionsPane } from "./RestoreVersions";
import { statusDot, useApp } from "./context";
import { Icon } from "./icons";
import SizeMap from "./SizeMap";
import { ClearButton, Sheet, Switch, useAct, useToast } from "./ui";
import { bytes, dayKey, dayLabel, longWhen, tilde } from "./format";

const DAYS_SHOWN = 10;

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
  // Opened from Search: that snapshot, and that file found and picked. From the Size map
  // (inFiles): that snapshot, with the folders down to it opened in Files instead.
  const focus = screen.name === "restore" ? screen.focus : undefined;
  const [query, setQuery] = useState(
    screen.name === "restore" ? (screen.query ?? (focus?.inFiles ? "" : focus?.path.split("/").pop()) ?? "") : "",
  );
  const shown = useRef(false);
  const [hits, setHits] = useState<Entry[] | null>(null);
  const [comparing, setComparing] = useState<{ snapshot: string; path: string } | null>(null);
  // The snapshot as a list of files, or as a map of what takes the space.
  const [view, setView] = useState<"files" | "map">((screen.name === "restore" && screen.view) || "files");
  // Shown from the map: that item, once the folders on the way to it are listed.
  const [reveal, setReveal] = useState<{ path: string } | null>(null);
  // Quick Look first copies the file out of the backup, which takes a while for a big one.
  const [looking, setLooking] = useState(false);
  // Folders stay first, as in Finder; within them, by the column clicked, again to reverse.
  const [sort, setSort] = useState<Sort>({ by: "name", up: true });

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
  // Bumped when such a job ends, to list the open folders again: a restore changes the Mac.
  const [fresh, setFresh] = useState(0);
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
        setFresh((n) => n + 1);
      },
      () => {},
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planJob]);

  const snap = snaps?.[sel];
  const snapId = snap?.id ?? "";
  const mapLoad = useCallback((path: string) => api.sizeMap(planId ?? "", snapId, path), [planId, snapId]);
  // Which listing the folders shown belong to. A folder asked for under another snapshot (or
  // before the deleted switch was flipped) that answers late is dropped, not shown under this one.
  const listing = useRef("");
  const listingOf = `${planId}\n${snapId}\n${showDeleted}`;
  useLayoutEffect(() => {
    listing.current = listingOf;
  }, [listingOf]);
  const load = useCallback(
    async (path: string) => {
      if (!planId || !snap) return;
      const of = `${planId}\n${snap.id}\n${showDeleted}`;
      const list = await api.listDir(planId, snap.id, path, showDeleted).catch((e) => {
        if (listing.current === of) setErr(String(e));
        return [] as Entry[];
      });
      if (listing.current !== of) return;
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
  useEffect(() => {
    if (!snap || !fresh) return;
    load("");
    open.forEach((p) => load(p));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fresh]);

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
    // Typing on while a search runs: only the newest search's answer is shown.
    let live = true;
    const t = window.setTimeout(
      () =>
        api.search(planId, snap.id, query.trim()).then(
          (h) => {
            if (!live) return;
            setHits(h);
            const f = focus && h.find((e) => e.path === focus.path);
            if (f) setPicked(f);
          },
          (e) => live && setErr(String(e)),
        ),
      250,
    );
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [query, planId, snap, focus]);

  const pickedFile = picked?.kind === "file" ? picked.path : undefined;
  useEffect(() => {
    setVersions(null);
    setVer(0);
    if (!pickedFile || !planId) return;
    // Another file picked before the answer comes: that one's versions are shown, not these. No
    // versions to be found reads as none, rather than Finding versions… for ever.
    let live = true;
    api.versions(planId, pickedFile).then(
      (v) => live && setVersions(v),
      () => live && setVersions([]),
    );
    return () => {
      live = false;
    };
  }, [pickedFile, planId]);

  const spaceLook = useRef<(() => void) | null>(null);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input, select, textarea") || !snaps) return;
      if (e.key === "ArrowLeft") setSel((s) => Math.max(0, s - 1));
      if (e.key === "ArrowRight") setSel((s) => Math.min(snaps.length - 1, s + 1));
      if (e.key === " " && !e.repeat && !e.metaKey && !e.ctrlKey && !e.altKey && spaceLook.current) {
        e.preventDefault();
        spaceLook.current();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [snaps]);

  // From the map to the list: open the folders down to it, then pick it.
  const showInFiles = (path: string) => {
    setView("files");
    setQuery("");
    // A source's name is its whole path, so the way down starts from the source it's in.
    const root = (kids[""] ?? []).find((r) => path === r.path || path.startsWith(`${r.path}/`));
    if (!root) return;
    const way = [root.path];
    for (const part of path
      .slice(root.path.length + 1)
      .split("/")
      .filter(Boolean))
      way.push(`${way[way.length - 1]}/${part}`);
    const s = new Set(open);
    for (const p of way) {
      s.add(p);
      if (!kids[p]) load(p);
    }
    setOpen(s);
    setReveal({ path });
  };
  // Opened from the Size map: show it once the focused snapshot's sources are listed.
  useEffect(() => {
    if (!focus?.inFiles || shown.current || snap?.id !== focus.snapshot || !kids[""]) return;
    shown.current = true;
    showInFiles(focus.path);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kids[""], snap?.id]);

  // The versions pane's Quick Look, also on Space while a file's version is showing and nothing
  // else is in the way (one of several chosen, it's already opening, or a comparison is open).
  const version = versions?.[ver];
  const lookAtVersion = async () => {
    if (!version || !picked || !planId) return;
    setLooking(true);
    await act(() => api.quickLook(planId, version.snapshot, picked.path));
    setLooking(false);
  };
  const canLook =
    view === "files" && picked?.kind === "file" && !!version && !looking && !comparing && !(checked.size > 1 && checked.has(picked.path));
  useEffect(() => {
    spaceLook.current = canLook ? lookAtVersion : null;
  });

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
  // A chosen folder brings everything in it, so what's inside shows as chosen too. Only what's
  // in this snapshot comes with it: a deleted item has to be chosen on its own.
  const comesWithFolder = (e: Entry) => e.tag !== "deleted" && !onlyOnMac(e);
  const chosenFolder = (e: Entry) =>
    comesWithFolder(e) ? [...checked.keys()].find((p) => p !== e.path && e.path.startsWith(`${p}/`)) : undefined;
  const isChosen = (e: Entry) => checked.has(e.path) || chosenFolder(e) !== undefined;
  // A folder with something chosen inside it, but not all of it.
  const partlyChosen = (e: Entry) => !isChosen(e) && [...checked.keys()].some((p) => p.startsWith(`${e.path}/`));
  const toggleCheck = (e: Entry) => {
    const m = new Map(checked);
    const folder = chosenFolder(e);
    if (m.has(e.path)) m.delete(e.path);
    else if (folder) {
      // Leaving one item out of a chosen folder: choose the rest of each folder on the way down
      // to it instead. They're all listed, since the item is showing.
      let at = folder;
      const way = e.path.slice(folder.length + 1).split("/");
      if (way.some((_, i) => !kids[i === 0 ? folder : `${folder}/${way.slice(0, i).join("/")}`])) {
        toast("Open the folders down to it to leave it out.");
        return;
      }
      m.delete(folder);
      for (const part of way) {
        const next = `${at}/${part}`;
        for (const k of kids[at] ?? []) if (k.path !== next && comesWithFolder(k)) m.set(k.path, k);
        at = next;
      }
    } else {
      // A folder chosen whole replaces anything chosen inside it, so nothing is restored twice.
      for (const p of m.keys()) if (p.startsWith(`${e.path}/`)) m.delete(p);
      m.set(e.path, e);
    }
    setChecked(m);
  };

  const items = [...checked.values()];
  // One of several chosen items: actions for one item at a time don't apply to it.
  const inMulti = (e: Entry) => checked.size > 1 && checked.has(e.path);
  // What a right-click acts on: all the chosen items when it's one of them, else just that row.
  const targets = (e: Entry) => (inMulti(e) ? items : [e]);
  const restoreItems = async (es: Entry[]) => {
    if (!snap) return;
    // A deleted item's last version is in the snapshot before this one.
    const here = es.filter((e) => e.tag !== "deleted").map((e) => e.path);
    const gone = es.filter((e) => e.tag === "deleted").map((e) => e.path);
    if (here.length && !(await start(snap.id, here))) return;
    if (gone.length && sel > 0) await start(snaps![sel - 1].id, gone);
  };
  const unchoose = (es: Entry[]) => {
    const m = new Map(checked);
    es.forEach((e) => m.delete(e.path));
    setChecked(m);
  };
  // The header's box: everything at the top of the list (a folder brings all that's in it), or
  // nothing. Choosing everything replaces what was chosen below the top, so nothing goes twice.
  const top = (hits ?? kids[""] ?? []).filter((e) => !onlyOnMac(e));
  const allChosen = top.length > 0 && top.every((e) => checked.has(e.path));
  const someChosen = checked.size > 0 && !allChosen;
  const chooseAll = () => setChecked(allChosen ? new Map() : new Map(top.map((e) => [e.path, e])));
  const total = items.reduce((n, e) => n + e.size, 0);
  // The original on the Mac, shown in its folder in Finder. An SMB share's files show no
  // "Not on Mac" until the share is connected, so Show in Finder finds out then.
  const onMac = (e: Entry) => e.path.startsWith("/") && e.disk !== "gone";
  // Connects an SMB share first, so it can take a moment; errors say why it can't be shown.
  const showInFinder = (e: Entry) => act(() => api.showInFinder(plan.id, e.path));
  // Whether the restore was started: when it wasn't, act's toast says why, and there's nothing to
  // back up afterwards or announce.
  const start = async (snapshot: string, paths: string[]): Promise<boolean> => {
    if (dest === "folder" && !folder) {
      toast("Choose a folder to restore into.");
      return false;
    }
    const target = dest === "folder" ? { kind: "folder" as const, path: folder } : { kind: "original" as const };
    if ((await act(() => api.restore(plan.id, snapshot, paths, target, conflict))) === undefined) return false;
    // Back in place, a restored file is on the Mac again; a backup after it puts that in a
    // snapshot, so it stops showing as deleted.
    if (dest === "original") await api.backUp(plan.id).catch(() => {});
    toast(`Restoring ${paths.length === 1 ? paths[0].split("/").pop() : `${paths.length} items`}. Activity shows how it's going.`);
    return true;
  };
  const chooseFolder = async () => {
    const [f] = await api.chooseFolders("Restore into this folder", false, folder || undefined);
    if (f) {
      setFolder(f);
      setDest("folder");
    }
  };
  // Deleting a file or folder from every snapshot: asked twice, as it can't be undone.
  const removeFromBackup = async (es: Entry[]) => {
    if (!es.length) return;
    const { ask } = await import("@tauri-apps/plugin-dialog");
    const one = es.length === 1;
    const where = one ? tilde(es[0].path, home) : `these ${es.length} items`;
    const first = await ask(
      one
        ? `Remove ${where} from every snapshot of ${plan.name}? Its space is freed, and none of its versions can be restored afterwards.`
        : `Remove ${where} from every snapshot of ${plan.name}? Their space is freed, and none of their versions can be restored afterwards.`,
      {
        title: one ? `Remove ${es[0].name} from the backup` : `Remove ${es.length} items from the backup`,
        kind: "warning",
        okLabel: "Continue",
        cancelLabel: "Cancel",
      },
    );
    if (!first) return;
    const sure = await ask(`Delete every backed-up version of ${where}? This can't be undone.`, {
      title: "Are you sure?",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (!sure) return;
    // Stops at the first that can't be removed (act's toast says why), and says only what was.
    let done = 0;
    for (const e of es) {
      if ((await act(() => api.removePathData(plan.id, e.path))) === undefined) break;
      done++;
    }
    if (!done) return;
    toast(
      `Removing ${done === 1 ? es[0].name : `${done} items`}${done < es.length ? ` of ${es.length}` : ""} from the backup. Activity shows how it's going.`,
    );
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
              onChange={(e) => {
                setQuery(e.target.value);
                setView("files");
              }}
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
          <div className="view-tabs">
            <div className="seg" role="tablist" aria-label="Show the snapshot as">
              <button role="tab" aria-selected={view === "files"} className={view === "files" ? "on" : ""} onClick={() => setView("files")}>
                Files
              </button>
              <button role="tab" aria-selected={view === "map"} className={view === "map" ? "on" : ""} onClick={() => setView("map")}>
                Size map
              </button>
            </div>
            <span className="small muted">
              {view === "files"
                ? "Every file and folder, with its versions."
                : "Each folder drawn as big as what it holds. Click one to zoom in, ⌘-click to find it in Files."}
            </span>
          </div>
        )}

        {snap && view === "map" && (
          <section className="card" style={{ flexGrow: 1, minHeight: 0, display: "flex", overflow: "hidden" }}>
            <SizeMap load={mapLoad} home={home} onShow={showInFiles} />
          </section>
        )}

        {snap && view === "files" && (
          <section className="card" style={{ flexGrow: 1, minHeight: 0, display: "flex", overflow: "hidden" }}>
            <FileList
              kids={kids}
              open={open}
              hits={hits}
              sort={sort}
              setSort={setSort}
              picked={picked}
              setPicked={setPicked}
              reveal={reveal}
              revealed={() => setReveal(null)}
              home={home}
              isChosen={isChosen}
              partlyChosen={partlyChosen}
              toggleCheck={toggleCheck}
              toggleOpen={toggleOpen}
              allChosen={allChosen}
              someChosen={someChosen}
              canChooseAll={top.length > 0}
              chooseAll={chooseAll}
              menu={(e) => (
                <RowMenu
                  e={e}
                  count={targets(e).length}
                  inSnapshot={e.tag !== "deleted" && !onlyOnMac(e)}
                  canRestore={targets(e).every((x) => !onlyOnMac(x))}
                  chosen={isChosen(e)}
                  canOpen={e.kind === "dir" && !hits && e.tag !== "deleted" && !onlyOnMac(e)}
                  isOpen={open.has(e.path)}
                  restoreTo={dest === "folder" ? (folder.split("/").pop() ?? "") : "where it was"}
                  onMac={onMac(e)}
                  quickLook={() => act(() => api.quickLook(plan.id, snap.id, e.path))}
                  compare={() => setComparing({ snapshot: snap.id, path: e.path })}
                  restore={() => restoreItems(targets(e))}
                  choose={() => (inMulti(e) ? unchoose(items) : toggleCheck(e))}
                  toggleOpen={() => toggleOpen(e.path)}
                  reveal={() => showInFinder(e)}
                  copyPath={() =>
                    act(() =>
                      navigator.clipboard.writeText(
                        targets(e)
                          .map((x) => x.path)
                          .join("\n"),
                      ),
                    )
                  }
                  remove={() => removeFromBackup(targets(e))}
                />
              )}
            />
            <VersionsPane
              picked={picked}
              home={home}
              snapId={snap.id}
              versions={versions}
              ver={ver}
              setVer={setVer}
              multi={!!picked && inMulti(picked)}
              onMac={!!picked && onMac(picked)}
              looking={looking}
              quickLook={lookAtVersion}
              compare={(v) => picked && setComparing({ snapshot: v.snapshot, path: picked.path })}
              restore={(v) => picked && start(v.snapshot, [picked.path])}
              remove={() => picked && removeFromBackup([picked])}
              reveal={() => picked && showInFinder(picked)}
            />
          </section>
        )}
      </div>

      {snap && (
        <RestoreFooter
          time={snap.time}
          restoring={restoring}
          queued={restoreQueued}
          count={items.length}
          total={total}
          dest={dest}
          setDest={setDest}
          folder={folder}
          chooseFolder={chooseFolder}
          conflict={conflict}
          setConflict={setConflict}
          home={home}
          restore={() => restoreItems(items)}
        />
      )}
      {comparing && <CompareSheet plan={plan.id} snapshot={comparing.snapshot} path={comparing.path} onClose={() => setComparing(null)} />}
    </>
  );
}
