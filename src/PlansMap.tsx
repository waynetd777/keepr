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

export default function PlansMap() {
  const { ov, home, go } = useApp();
  // Each plan with a backup, and its latest snapshot.
  const [latest, setLatest] = useState<{ id: string; name: string; snapshot: string }[] | null>(null);
  const [err, setErr] = useState("");
  const kept = (ov?.plans ?? []).filter((p) => p.snapshots > 0);
  const key = kept.map((p) => `${p.id}:${p.snapshots}:${p.lastSuccess}`).join(",");

  useEffect(() => {
    let live = true;
    Promise.all(
      kept.map((p) => api.snapshots(p.id).then((s) => (s.length ? { id: p.id, name: p.name, snapshot: s[s.length - 1].id } : null))),
    ).then(
      (l) => live && setLatest(l.filter((x) => x !== null)),
      (e) => live && setErr(String(e)),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const split = (path: string) => {
    const i = path.indexOf(SEP);
    const plan = latest?.find((p) => p.id === path.slice(0, i));
    return { plan, inner: path.slice(i + 1) };
  };

  const load = useCallback(
    async (path: string): Promise<MapDir> => {
      const plans = latest ?? [];
      if (!path) {
        // One plan: straight to its folders.
        if (plans.length === 1) {
          const p = plans[0];
          return under(await api.sizeMap(p.id, p.snapshot, ""), p.id, 0);
        }
        const kids = (
          await Promise.all(plans.map(async (p) => ({ ...under(await api.sizeMap(p.id, p.snapshot, ""), p.id, 1), name: p.name })))
        ).sort((a, b) => b.size - a.size);
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
      const p = plans.find((x) => x.id === path.slice(0, i));
      if (!p) throw new Error("That plan has no backup any more.");
      const inner = path.slice(i + 1);
      const d = under(await api.sizeMap(p.id, p.snapshot, inner), p.id, 0);
      return inner ? d : { ...d, name: p.name };
    },
    [latest],
  );

  const where = (path: string) => {
    if (!path) return "";
    const { plan, inner } = split(path);
    return [plan?.name, inner && tilde(inner, home)].filter(Boolean).join(" › ");
  };
  const show = (path: string) => {
    const { plan, inner } = split(path);
    if (!plan) return;
    if (!inner) go({ name: "restore", plan: plan.id, view: "map" });
    else go({ name: "restore", plan: plan.id, focus: { snapshot: plan.snapshot, path: inner } });
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
      {err ? (
        <div className="banner bad">
          <Icon name="warning" size={18} />
          <span className="text">{err}</span>
        </div>
      ) : latest && latest.length === 0 ? (
        <div className="empty">Once a plan has backed up, its map is here.</div>
      ) : (
        <section className="card" style={{ flexGrow: 1, minHeight: 0, display: "flex", overflow: "hidden" }}>
          {latest && (
            <SizeMap
              load={load}
              home={home}
              onShow={show}
              top={latest.length === 1 ? latest[0].name : "All plans"}
              where={where}
              showHint="open in Restore"
            />
          )}
        </section>
      )}
    </div>
  );
}
