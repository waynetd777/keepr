// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Size map screen, from Backup Plans' Space used card: every plan's latest snapshot in one
// size map, a block per plan. Click a plan to zoom into its folders; ⌘-click opens it in Restore.

import { useCallback, useEffect, useState } from "react";
import { api, type MapDir } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import SizeMap from "./SizeMap";
import { tilde } from "./format";

// A path on this map is the plan's id, this, then the path in its snapshot.
const SEP = "\t";

/// A plan's map with its paths made the map's, and its levels moved down by `shift`.
function under(d: MapDir, plan: string, shift: number): MapDir {
  return { ...d, path: `${plan}${SEP}${d.path}`, depth: d.depth + shift, kids: d.kids.map((k) => under(k, plan, shift)) };
}

/// One plan on the map: its latest snapshot and top-level map once read, or why it couldn't be.
type Part = { id: string; name: string; snapshot?: string; map?: MapDir; err?: string };

export default function PlansMap() {
  const { ov, home, go } = useApp();
  const kept = (ov?.plans ?? []).filter((p) => p.snapshots > 0);
  const key = kept.map((p) => `${p.id}:${p.snapshots}:${p.lastSuccess}`).join(",");
  // Each plan is read on its own, so one that's slow (a share to connect, a password to ask
  // for) or broken doesn't hold up the rest: the map draws what has come in so far.
  const [parts, setParts] = useState<Part[]>([]);

  useEffect(() => {
    let live = true;
    setParts(kept.map((p) => ({ id: p.id, name: p.name })));
    const put = (id: string, part: Partial<Part>) => live && setParts((ps) => ps.map((x) => (x.id === id ? { ...x, ...part } : x)));
    for (const p of kept) {
      api
        .snapshots(p.id)
        .then(async (s) => {
          const snapshot = s[s.length - 1]?.id;
          if (!snapshot) throw "It has no snapshots yet.";
          put(p.id, { snapshot, map: await api.sizeMap(p.id, snapshot, "") });
        })
        .catch((e) => put(p.id, { err: String(e) }));
    }
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const ready = parts.filter((p) => p.map);
  const waiting = parts.filter((p) => !p.map && !p.err);
  const failed = parts.filter((p) => p.err);
  // The top changes as plans come in; below it, a plan's folders are read when zoomed into.
  const readyKey = ready.map((p) => p.id).join(",");

  const split = (path: string) => {
    const i = path.indexOf(SEP);
    const plan = parts.find((p) => p.id === path.slice(0, i));
    return { plan, inner: path.slice(i + 1) };
  };

  const load = useCallback(
    async (path: string): Promise<MapDir> => {
      if (!path) {
        // Nothing in yet: SizeMap shows it's working until a plan comes in and this is remade.
        if (!ready.length) return new Promise<MapDir>(() => {});
        // One plan: straight to its folders.
        if (parts.length === 1) {
          const p = ready[0];
          return { ...under(p.map!, p.id, 0), name: p.name };
        }
        const kids = ready.map((p) => ({ ...under(p.map!, p.id, 1), name: p.name })).sort((a, b) => b.size - a.size);
        return {
          name: "",
          path: "",
          size: kids.reduce((n, k) => n + k.size, 0),
          files: kids.reduce((n, k) => n + k.files, 0),
          depth: 0,
          looseFiles: 0,
          looseSize: 0,
          more: 0,
          moreSize: 0,
          read: true,
          kids,
        };
      }
      const i = path.indexOf(SEP);
      const p = parts.find((x) => x.id === path.slice(0, i));
      if (!p?.snapshot) throw new Error("That plan has no backup any more.");
      const inner = path.slice(i + 1);
      const d = under(await api.sizeMap(p.id, p.snapshot, inner), p.id, 0);
      return inner ? d : { ...d, name: p.name };
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [readyKey],
  );

  const where = (path: string) => {
    if (!path) return "";
    const { plan, inner } = split(path);
    return [plan?.name, inner && tilde(inner, home)].filter(Boolean).join(" › ");
  };
  const show = (path: string) => {
    const { plan, inner } = split(path);
    if (!plan?.snapshot) return;
    if (!inner) go({ name: "restore", plan: plan.id, view: "map" });
    else go({ name: "restore", plan: plan.id, focus: { snapshot: plan.snapshot, path: inner, inFiles: true } });
  };

  return (
    <div className="content col" style={{ gap: 16, padding: "20px 32px 18px", overflow: "hidden" }}>
      <div className="col" style={{ gap: 4 }}>
        <h1>Size map</h1>
        <span className="small muted">
          Each plan's latest backup, its folders drawn as big as the files they hold. Sizes are of the files themselves, before
          de-duplication and compression, so they won't match the space used. Click to zoom in, ⌘-click to open in Restore.
        </span>
      </div>
      {(waiting.length > 0 || failed.length > 0) && (
        <div className="col small muted" style={{ gap: 4 }}>
          {waiting.length > 0 && (
            <span className="row" style={{ gap: 8 }}>
              <span className="dot spin" style={{ width: 10, height: 10 }} />
              Reading {waiting.map((p) => p.name).join(", ")}…
            </span>
          )}
          {failed.map((p) => (
            <span key={p.id} className="row" style={{ gap: 8, color: "var(--amber)" }}>
              <Icon name="warning" size={14} />
              {p.name} isn't on the map: {p.err}
            </span>
          ))}
        </div>
      )}
      {kept.length === 0 ? (
        <div className="empty">Once a plan has backed up, its map is here.</div>
      ) : failed.length === parts.length && parts.length > 0 ? null : (
        <section className="card" style={{ flexGrow: 1, minHeight: 0, display: "flex", overflow: "hidden" }}>
          <SizeMap
            load={load}
            home={home}
            onShow={show}
            top={parts.length === 1 ? parts[0].name : "All plans"}
            where={where}
            showHint="open in Restore"
          />
        </section>
      )}
    </div>
  );
}
