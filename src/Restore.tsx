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
import { ClearButton, confirm, Sheet, stopped, Switch, tell, useAct, useRequests, useSlowToast, useToast } from "./ui";
import { bytes, count, dayKey, dayLabel, longWhen, tilde } from "./format";

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
  // As many days as fit the strip's width, up to ten, so it never scrolls: the picked day, then
  // the days after it, then those before. A busy day's bars get thinner rather than wider.
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const selDay = Math.max(
    0,
    days.findIndex((d) => d.items.includes(sel)),
  );
  const need = (d: { items: number[] }) => Math.max(64, d.items.length * 5 + 17);
  let start = selDay;
  let end = selDay + 1;
  let room = width - 88 - need(days[selDay] ?? { items: [] });
  while (end - start < DAYS_SHOWN && end < days.length && room >= need(days[end])) room -= need(days[end++]);
  while (end - start < DAYS_SHOWN && start > 0 && room >= need(days[start - 1])) room -= need(days[--start]);
  const shown = days.slice(start, end);
  const olderCount = days.slice(0, start).reduce((n, d) => n + d.items.length, 0);
  const max = Math.max(1, ...snaps.map((s) => s.addedBytes));
  return (
    <div className="snapstrip" ref={ref}>
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
        <div key={d.key} className="snapday" style={{ minWidth: need(d) }}>
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
  const { ov, screen } = useApp();
  const plans = ov?.plans ?? [];
  const planId = (screen.name === "restore" && screen.plan) || plans.find((p) => p.snapshots > 0)?.id || plans[0]?.id;
  // Opened with no plan named, the plan shown can change under it (the first to back up, say):
  // everything ticked, picked and opened was of the other plan, so it starts again.
  return <RestoreOf key={planId ?? ""} planId={planId} />;
}

function RestoreOf({ planId }: { planId: string | undefined }) {
  const { ov, cfg, screen, home, go } = useApp();
  const act = useAct();
  const toast = useToast();
  // Reading folders, versions, a search, a Quick Look copy: stopped when Restore goes, or when a
  // newer one of the same kind replaces it.
  const req = useRequests();
  const slowToast = useSlowToast();
  const plans = ov?.plans ?? [];
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
  // On to begin with, so what the Size map sends here shows even if it has been deleted since.
  const [showDeleted, setShowDeleted] = useState(true);
  const [dest, setDest] = useState<"original" | "folder">("original");
  const [folder, setFolder] = useState("");
  const [conflict, setConflict] = useState<Conflict>("keepBoth");
  // Opened from Search or the Size map: that snapshot, with the folders down to that item opened
  // in Files and it picked, once. (A search by its name could miss it: a search stops at 200.)
  const focus = screen.name === "restore" ? screen.focus : undefined;
  const [query, setQuery] = useState(screen.name === "restore" ? (screen.query ?? "") : "");
  const shown = useRef(false);
  const [hits, setHits] = useState<Entry[] | null>(null);
  const [comparing, setComparing] = useState<{ snapshot: string; path: string } | null>(null);
  // The snapshot as a list of files, or as a map of what takes the space.
  const [view, setView] = useState<"files" | "map">((screen.name === "restore" && screen.view) || "files");
  // Shown from the map: that item, once the folders on the way to it are listed.
  const [reveal, setReveal] = useState<{ path: string } | null>(null);
  // Quick Look first copies the file out of the backup, which takes a while for a big one. Each
  // has a number, so one finishing doesn't clear the spinner while a later one is still copying.
  const [looking, setLooking] = useState(false);
  const lookNo = useRef(0);
  // Folders stay first, as in Finder; within them, by the column clicked, again to reverse.
  const [sort, setSort] = useState<Sort>({ by: "name", up: true });

  useEffect(() => {
    if (!planId) return;
    setSnaps(null);
    setErr("");
    // Another plan picked before the answer comes: that plan's snapshots are shown, not these.
    let live = true;
    api.snapshots(planId).then(
      (s) => {
        if (!live) return;
        setSnaps(s);
        const at = focus ? s.findIndex((x) => x.id === focus.snapshot) : -1;
        setSel(at >= 0 ? at : s.length - 1);
      },
      (e) => live && setErr(String(e)),
    );
    return () => {
      live = false;
    };
  }, [planId, focus]);

  // A job of this plan's has ended: a backup adds a snapshot and Remove from backup rewrites them,
  // so fetch them again, staying on the same moment (or the newest, if that's where we were).
  const planJob = planId && ov?.job?.plan === planId ? ov.job.id : undefined;
  const lastJob = useRef(planJob);
  // Bumped when such a job ends, to list the open folders again: a restore changes the Mac.
  const [fresh, setFresh] = useState(0);
  // Bumped when snapshots have gone (rewritten or tidied away), to find the versions again.
  const [pruned, setPruned] = useState(0);
  // The moment shown when the answer comes, not when it was asked for: one picked meanwhile stays.
  // And what's ticked and picked now, for what's done after a wait (Remove from backup's two
  // questions, a job's end), not as they were when it started.
  const now = useRef({ snaps, sel, checked, picked });
  useLayoutEffect(() => {
    now.current = { snaps, sel, checked, picked };
  });
  // Unticks what `out` says of the ticks as they are now. The pane goes to the last item still
  // ticked, or to none, if it was on one of them or inside a folder unticked: it's never left on
  // something no longer chosen while other things are, where its Remove from backup would be
  // about the wrong thing. Says how many were unticked.
  const untick = (out: (e: Entry) => boolean): number => {
    const keep = (m: Map<string, Entry>) => new Map([...m].filter(([, e]) => !out(e)));
    const was = now.current.checked;
    const rest = keep(was);
    const gone = [...was.values()].filter(out);
    setChecked((m) => keep(m));
    const off = (p: Entry) => out(p) || gone.some((e) => p.path.startsWith(`${e.path}/`));
    setPicked((p) => (p && off(p) ? ([...rest.values()].pop() ?? null) : p));
    return gone.length;
  };
  // Says why something ticked isn't any more, instead of "Another snapshot", when the snapshot
  // changes for a reason other than the user picking one.
  const snapNote = useRef<string | null>(null);
  useEffect(() => {
    const ended = lastJob.current && lastJob.current !== planJob;
    lastJob.current = planJob;
    if (!ended || !planId || !snaps) return;
    let live = true;
    api.snapshots(planId).then(
      (s) => {
        const { snaps: was, sel: at0, checked: ticks } = now.current;
        if (!live || !was) return;
        const ids = new Set(s.map((x) => x.id));
        const shownId = was[at0]?.id;
        // A backup adds a snapshot and leaves the others as they were: the one shown stays, and
        // so does what's ticked in it. At the newest, the new one is shown instead, unless
        // something is ticked, which would be lost.
        let at = s.findIndex((x) => x.id === shownId);
        if (at >= 0 && at0 === was.length - 1 && !ticks.size) at = s.length - 1;
        if (at < 0) {
          // Rewritten (Remove from backup) or tidied away: the one from the same moment, or the
          // newest. Nothing ticked in the old one stays ticked; the pane finds its item again.
          const t = s.findIndex((x) => x.time === was[at0]?.time);
          at = t >= 0 ? t : s.length - 1;
          snapNote.current = "The snapshots were rewritten: nothing is ticked any more.";
        } else {
          // A deleted item restores from its last snapshot: one that has gone can't be.
          const n = untick((e) => e.tag === "deleted" && !!e.lastSnapshot && !ids.has(e.lastSnapshot));
          if (n) toast(`The snapshots were rewritten: ${count(n, "deleted item")} ticked ${n === 1 ? "isn't" : "aren't"} any more.`);
        }
        // Every snapshot gone: nothing is shown to clear the ticks with, so they go here.
        if (!s.length) untick(() => true);
        setSnaps(s);
        setSel(at);
        // The versions listed may name a snapshot that has gone.
        if (was.some((x) => !ids.has(x.id))) setPruned((n) => n + 1);
        setFresh((n) => n + 1);
      },
      () => {},
    );
    return () => {
      // Another job ended before the answer came: the newer answer is the one shown.
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planJob]);

  const snap = snaps?.[sel];
  const snapId = snap?.id ?? "";
  const mapLoad = useCallback((path: string) => req.run((t) => api.sizeMap(planId ?? "", snapId, path, t), "map"), [planId, snapId, req]);
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
      let gaveUp = false;
      const list = await req
        .run((t) => api.listDir(planId, snap.id, path, showDeleted, t))
        .catch((e) => {
          // Stopped (Restore left, or the snapshot changed): nothing to show or say.
          gaveUp = stopped(e);
          if (listing.current === of && !gaveUp) setErr(String(e));
          return [] as Entry[];
        });
      if (listing.current !== of || gaveUp) return;
      setKids((k) => ({ ...k, [path]: list }));
    },
    [planId, snap, showDeleted, req],
  );

  // Another moment: what was ticked was ticked as it was then (a file that isn't in this one, a
  // deleted item's last snapshot), so nothing stays ticked; the pane stays only on an item this
  // moment has too, once the folder it's in is listed again.
  const recheck = useRef(false);
  const hitsAt = useRef("");
  useEffect(() => {
    if (!snapId) return;
    if (checked.size) toast(snapNote.current ?? "Another snapshot: nothing is ticked any more.");
    snapNote.current = null;
    setChecked(new Map());
    // What the last one's search found isn't in this one to tick; the search runs again.
    setHits(null);
    recheck.current = !!picked;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapId]);
  useEffect(() => {
    if (!recheck.current || !picked) return;
    // Results found in the moment before don't count; the search runs again for this one.
    const lists = [...(hitsAt.current === snapId ? (hits ?? []) : []), ...Object.values(kids).flat()];
    const there = lists.find((e) => e.path === picked.path);
    if (there) {
      recheck.current = false;
      setPicked(there);
      return;
    }
    // Not listed yet, or listed and not there: the folder it's in (the sources, for a source).
    const parent = picked.path.slice(0, picked.path.lastIndexOf("/")) || "/";
    const inRoot = (kids[""] ?? []).some((r) => picked.path.startsWith(`${r.path}/`));
    const where = kids[parent] ?? (inRoot ? undefined : kids[""]);
    // While searching, the results are what's showing: not among them, it isn't shown.
    if (where || (hits && hitsAt.current === snapId)) {
      recheck.current = false;
      setPicked(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kids, hits]);

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

  // Searching this snapshot, for the list to say so.
  const [searching, setSearching] = useState(false);
  useEffect(() => {
    if (!planId || !snap || !query.trim()) {
      req.stop("search");
      setHits(null);
      setSearching(false);
      return;
    }
    // Typing on while a search runs: the older one is stopped, only the newest's answer shown.
    let live = true;
    const t = window.setTimeout(() => {
      setSearching(true);
      req
        .run((tk) => api.search(planId, snap.id, query.trim(), tk), "search")
        .then(
          (h) => {
            if (!live) return;
            hitsAt.current = snap.id;
            setHits(h);
          },
          (e) => live && !stopped(e) && setErr(String(e)),
        )
        .finally(() => live && setSearching(false));
    }, 250);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [query, planId, snap, req]);

  const pickedFile = picked?.kind === "file" ? picked.path : undefined;
  useEffect(() => {
    setVersions(null);
    setVer(0);
    if (!pickedFile || !planId) return;
    // Another file picked before the answer comes: that one's versions are shown, not these. No
    // versions to be found reads as none, rather than Finding versions… for ever.
    let live = true;
    req
      .run((t) => api.versions(planId, pickedFile, t), "versions")
      .then(
        (v) => live && setVersions(v),
        () => live && setVersions([]),
      );
    return () => {
      live = false;
    };
  }, [pickedFile, planId, req, pruned]);

  const spaceLook = useRef<(() => void) | null>(null);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input, select, textarea") || !snaps) return;
      // A dialog, sheet or menu is up (Remove from backup's question, a comparison, a row's
      // menu, which acts on the snapshot it was opened in): the keys are its.
      if (document.querySelector(".scrim, .menu")) return;
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
  // Opened from Search or the Size map: show it once the focused snapshot's sources are listed
  // (the newest's, if that snapshot has gone since).
  const focusAt = focus && snaps && (snaps.some((x) => x.id === focus.snapshot) ? focus.snapshot : snaps[snaps.length - 1]?.id);
  useEffect(() => {
    if (!focus || shown.current || !snap || snap.id !== focusAt || !kids[""]) return;
    shown.current = true;
    showInFiles(focus.path);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kids[""], snap?.id]);

  // The versions pane's Quick Look, also on Space while a file's version is showing and nothing
  // else is in the way (one of several chosen, it's already opening, or a comparison is open).
  const version = versions?.[ver];
  // A Quick Look copies the file out of the backup first, which can take a while for a big one
  // on a remote destination: the versions pane says so, with Cancel, and leaving stops it.
  const lookAt = async (snapshot: string, e: Entry) => {
    if (!planId) return;
    const no = ++lookNo.current;
    setPicked(e);
    setLooking(true);
    await req
      .run((t) => api.quickLook(planId, snapshot, e.path, t), "look")
      .catch((err) => !stopped(err) && no === lookNo.current && toast(String(err)));
    if (no === lookNo.current) setLooking(false);
  };
  const stopLooking = () => {
    req.stop("look");
    setLooking(false);
  };
  const lookAtVersion = async () => {
    if (!version || !picked) return;
    await lookAt(version.snapshot, picked);
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
    // Whether the pane is on what's no longer chosen: the item, or (for a folder) what's in it.
    let paneOff = (p: Entry) => p.path === e.path || p.path.startsWith(`${e.path}/`);
    if (m.has(e.path)) {
      m.delete(e.path);
    } else if (folder) {
      // Leaving one item out of a chosen folder: choose the rest of each folder on the way down
      // to it instead. They're all listed, since the item is showing.
      let at = folder;
      const way = e.path.slice(folder.length + 1).split("/");
      if (way.some((_, i) => !kids[i === 0 ? folder : `${folder}/${way.slice(0, i).join("/")}`])) {
        toast("Open the folders down to it to leave it out.");
        return;
      }
      // Nor is it left on the folder the item was left out of, or one between them: that folder
      // isn't chosen any more, and its Remove from backup would take the item left out with it.
      paneOff = (p: Entry) => p.path === e.path || e.path.startsWith(`${p.path}/`);
      m.delete(folder);
      for (const part of way) {
        const next = `${at}/${part}`;
        for (const k of kids[at] ?? []) if (k.path !== next && comesWithFolder(k)) m.set(k.path, k);
        at = next;
      }
    } else {
      // Ticking an item picks it too, so the pane beside the list is about what was just ticked.
      setPicked(e);
      // A folder chosen whole replaces anything chosen inside it, so nothing is restored twice.
      for (const p of m.keys()) if (p.startsWith(`${e.path}/`)) m.delete(p);
      m.set(e.path, e);
      paneOff = () => false;
    }
    // The pane goes to the last of those still ticked, or to none, so its Remove from backup is
    // never about what was just unticked.
    if (picked && paneOff(picked)) setPicked([...m.values()].pop() ?? null);
    setChecked(m);
  };

  const items = [...checked.values()];
  // One of several chosen items: actions for one item at a time don't apply to it.
  const inMulti = (e: Entry) => checked.size > 1 && checked.has(e.path);
  // What a right-click acts on: all the chosen items when it's one of them, else just that row.
  const targets = (e: Entry) => (inMulti(e) ? items : [e]);
  const restoreItems = async (es: Entry[]) => {
    if (!snap) return;
    // A deleted item's last version is in the snapshot that says so: the one before this, or one
    // long before for something deleted since.
    const here = es.filter((e) => e.tag !== "deleted").map((e) => e.path);
    if (here.length && !(await start(snap.id, here))) return;
    const gone = new Map<string, string[]>();
    for (const e of es.filter((e) => e.tag === "deleted")) {
      const at = e.lastSnapshot ?? (sel > 0 ? snaps![sel - 1].id : undefined);
      if (at) gone.set(at, [...(gone.get(at) ?? []), e.path]);
    }
    for (const [at, paths] of gone) if (!(await start(at, paths))) return;
  };
  // Unticks these, as the ticks are now. `within`: what's ticked inside them too (they've been
  // removed from the backup, so nothing in them is left to restore or remove).
  const unchoose = (es: Entry[], within = false) => {
    const paths = new Set(es.map((e) => e.path));
    untick((e) => paths.has(e.path) || (within && es.some((x) => e.path.startsWith(`${x.path}/`))));
  };
  // The header's box: everything at the top of the list (a folder brings all that's in it), or
  // nothing. Choosing everything replaces what was chosen inside it, so nothing goes twice.
  const top = (hits ?? kids[""] ?? []).filter((e) => !onlyOnMac(e));
  const allChosen = top.length > 0 && top.every(isChosen);
  const someChosen = checked.size > 0 && !allChosen;
  const chooseAll = () => {
    if (!hits) {
      if (allChosen) unchoose(items);
      else {
        const m = new Map(top.map((e) => [e.path, e]));
        // A deleted item doesn't come with its folder, so one ticked on its own stays ticked.
        for (const [p, e] of checked) if (!comesWithFolder(e)) m.set(p, e);
        setChecked(m);
      }
      return;
    }
    // Search results: only they are ticked or unticked; what's ticked elsewhere stays as it is.
    const under = (p: string, m: Map<string, Entry>) => [...m.keys()].some((q) => q !== p && p.startsWith(`${q}/`));
    if (allChosen) {
      // A result shows as ticked because a folder it's in is: that folder is unticked with it.
      const out = new Map<string, Entry>();
      for (const e of top) {
        if (checked.has(e.path)) out.set(e.path, e);
        const f = chosenFolder(e);
        if (f) out.set(f, checked.get(f)!);
      }
      unchoose([...out.values()]);
      return;
    }
    const m = new Map(checked);
    for (const e of top) if (!isChosen(e)) m.set(e.path, e);
    // A result inside another ticked folder (one of the results, or ticked before) comes with it,
    // so it isn't restored or removed twice.
    for (const [p, e] of m) if (comesWithFolder(e) && under(p, m)) m.delete(p);
    setChecked(m);
  };
  const total = items.reduce((n, e) => n + e.size, 0);
  // The original on the Mac, shown in its folder in Finder. An SMB share's files show no
  // "Not on Mac" until the share is connected, so Show in Finder finds out then.
  const onMac = (e: Entry) => e.path.startsWith("/") && e.disk !== "gone";
  // Connects an SMB share first, so it can take a moment; errors say why it can't be shown.
  // Connecting its share first can take a moment, with nothing else to show for it.
  const showInFinder = (e: Entry) => act(() => slowToast(api.showInFinder(plan.id, e.path), `Opening ${e.name} in Finder…`));
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
  // What's being removed, as "2 files from ~/Desktop" or "the folder Old from ~/Desktop and
  // everything in it", so it's plain that only those go, not the folder they're in.
  const describe = (es: Entry[]) => {
    const parentOf = (p: string) => p.slice(0, p.lastIndexOf("/")) || "/";
    const parents = new Set(es.map((e) => parentOf(e.path)));
    const from = parents.size === 1 ? ` from ${tilde([...parents][0], home)}` : "";
    if (es.length === 1) {
      const e = es[0];
      return e.kind === "dir" ? `the folder ${e.name}${from} and everything in it` : `${e.name}${from}`;
    }
    const dirs = es.filter((e) => e.kind === "dir").length;
    const files = es.length - dirs;
    const what = [dirs && count(dirs, "folder"), files && count(files, "file")].filter(Boolean).join(" and ");
    return `${what}${from || " from different folders"}${dirs ? ", with everything in the folders" : ""}`;
  };
  // Each item by name, for a question about several: the first five, then how many more.
  const named = (es: Entry[]) => {
    const names = es.slice(0, 5).map((e) => tilde(e.path, home));
    const more = es.length - names.length;
    return more
      ? `${names.join(", ")} and ${more.toLocaleString()} more`
      : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  };
  // Whether a row is one of the sources the plan still backs up. A source's row is named as the
  // snapshots name it (src-tauri/src/core.rs remove_source): a folder by its path, a bucket as
  // s3://bucket/prefix, a share by where it's mounted, /Volumes/<share>[-n]/<folder>.
  const sourceOf = (e: Entry) => {
    const same = (a: string) => a.replace(/\/+$/, "") === e.path.replace(/\/+$/, "");
    const roots = kids[""] ?? [];
    const isRoot = roots.length === 0 || roots.some((r) => r.path === e.path);
    for (const s of cfg?.plans.find((p) => p.id === plan.id)?.sources ?? []) {
      if (s.kind === "folder" && same(s.path)) return "folder";
      if (!isRoot) continue;
      const prefix = s.kind === "s3" ? s.prefix.replace(/^\/+|\/+$/g, "") : "";
      if (s.kind === "s3" && same(`s3://${s.bucket.trim()}${prefix ? `/${prefix}` : ""}`)) return "bucket";
      if (s.kind === "smb") {
        const [, vol, mount, ...rest] = e.path.split("/");
        // Where macOS mounts it, by the share's name (-1 and on when that's taken), in any case.
        const share = s.share.replace(/^\/+|\/+$/g, "").toLowerCase();
        const sub = s.folder.replace(/^\/+|\/+$/g, "");
        const m = mount?.toLowerCase() ?? "";
        if (vol === "Volumes" && (m === share || /^-\d+$/.test(m.slice(share.length))) && m.startsWith(share) && rest.join("/") === sub)
          return "share";
      }
    }
    return undefined;
  };
  // Deleting a file or folder from every snapshot: asked twice, as it can't be undone.
  const removeFromBackup = async (es: Entry[]) => {
    if (!es.length) return;
    // A source the plan still backs up goes by taking it out of the plan, not from here (Rust
    // refuses it too). One already taken out of the plan can go from here.
    // A folder not chosen with some of what's in it ticked: removing it takes all of it, ticked or
    // left out, which isn't what the ticks say. Done with the folder ticked whole, or the ticks.
    const mixed = es.find((e) => e.kind === "dir" && partlyChosen(e));
    if (mixed) {
      const name = mixed.name.startsWith("/") ? tilde(mixed.name, home) : mixed.name;
      await tell(
        `Some of what's in ${name} is ticked`,
        `Remove from backup takes all of ${name}, ticked or not. To remove only what's ticked, right-click one of the ticked items; to remove all of ${name}, tick it first.`,
      );
      return;
    }
    const source = es.find(sourceOf);
    if (source) {
      const name = source.path.startsWith("/") ? tilde(source.path, home) : source.path;
      await tell(
        `${name} is a ${sourceOf(source)} this plan backs up`,
        `Remove from backup takes files and folders inside it out of the backup. To stop backing up ${name} and delete what's stored, take it out of what the plan backs up.`,
      );
      return;
    }
    const one = es.length === 1;
    const what = describe(es);
    // Several: each by name too, so nothing goes that isn't there in the question.
    const which = one ? "" : named(es);
    const first = await confirm({
      title: `Remove ${one ? es[0].name : describe(es).split(" from ")[0]} from the backup`,
      text: `Remove ${what} from every snapshot of ${plan.name}? ${which ? `That's ${which}. ` : ""}Only ${one ? "it goes" : "they go"}; nothing else is touched. ${one ? "Its" : "Their"} space is freed, and none of ${one ? "its" : "their"} versions can be restored afterwards.`,
      ok: "Continue",
    });
    if (!first) return;
    const sure = await confirm({
      title: "Are you sure?",
      text: `Delete every backed-up version of ${what}${which ? ` (${which})` : ""}? This can't be undone.`,
      ok: "Delete",
      danger: true,
    });
    if (!sure) return;
    // Stops at the first that can't be removed (act's toast says why), and says only what was.
    let done = 0;
    for (const e of es) {
      if ((await act(() => api.removePathData(plan.id, e.path))) === undefined) break;
      done++;
    }
    if (!done) return;
    // What's on its way out is no longer ticked (nor anything in it), so the folders it's in don't
    // show as partly chosen and it isn't restored or removed again.
    unchoose(es.slice(0, done), true);
    toast(`Removing ${done < es.length ? `${done} of ${es.length} items` : describe(es)} from the backup. Activity shows how it's going.`);
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
            <Switch
              label="Show deleted files"
              on={showDeleted}
              onChange={(on) => {
                setShowDeleted(on);
                // Hidden, a deleted item isn't left ticked (or in the pane) to be restored unseen.
                const n = on ? 0 : untick((e) => e.tag === "deleted");
                if (n) toast(`Deleted files hidden: ${count(n, "deleted item")} ${n === 1 ? "isn't" : "aren't"} ticked any more.`);
              }}
            />
          </label>
        </div>

        {err && (
          <div className="banner bad">
            <Icon name="warning" size={18} />
            <span className="text">{err}</span>
          </div>
        )}
        {snaps && snaps.length === 0 && !err && <div className="empty">{plan.name} hasn't finished a backup yet.</div>}
        {!snaps && !err && (
          <div className="empty row" style={{ gap: 8, justifyContent: "center" }}>
            <span className="dot spin" style={{ width: 12, height: 12 }} />
            Reading {plan.name}'s snapshots…
          </div>
        )}

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
              snapId={snap.id}
              kids={kids}
              open={open}
              hits={hits}
              searching={searching}
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
                  canOpen={e.kind === "dir" && !hits && !onlyOnMac(e)}
                  isOpen={open.has(e.path)}
                  restoreTo={dest === "folder" ? (folder.split("/").pop() ?? "") : "where it was"}
                  onMac={onMac(e)}
                  quickLook={() => void lookAt(snap.id, e)}
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
              stopLooking={stopLooking}
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
