// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The size map: folders drawn as nested blocks, each as big as what it holds, so what takes
// the space is plain at a glance. The layout is the squarified treemap from Disk Usage
// Visualiser. Click a folder to zoom into it; ⌘-click, or a click on a block of files, shows
// it. Restore draws one snapshot with it; the Size map screen draws every plan's latest.

import { useEffect, useRef, useState } from "react";
import type { MapDir } from "./api";
import { bytes, count, tilde } from "./format";
import { Icon } from "./icons";

/// What one block stands for: a folder, the files directly in a folder, or the folders of a
/// folder too small to draw, so a folder's area is always its whole size.
type Block = { kind: "dir" | "files" | "more"; dir: MapDir };
type Item = { block: Block; size: number };
type Rect = { item: Item; x: number; y: number; w: number; h: number };

const GAP = 3;
const HEAD = 20;
const PAD = 3;

/// Squarified treemap (Bruls, Huizing, van Wijk). Items sorted by size, biggest first.
function squarify(items: Item[], w: number, h: number): Rect[] {
  const rects: Rect[] = [];
  const total = items.reduce((a, b) => a + b.size, 0);
  if (total <= 0 || w <= 0 || h <= 0) return rects;
  const scale = (w * h) / total;
  let rx = 0,
    ry = 0,
    rw = w,
    rh = h;
  let row: { item: Item; a: number }[] = [];
  const worst = (r: { a: number }[], side: number) => {
    let s = 0,
      mx = 0,
      mn = Infinity;
    for (const i of r) {
      s += i.a;
      mx = Math.max(mx, i.a);
      mn = Math.min(mn, i.a);
    }
    return Math.max((side * side * mx) / (s * s), (s * s) / (side * side * mn));
  };
  const layout = (r: { item: Item; a: number }[]) => {
    const s = r.reduce((a, b) => a + b.a, 0);
    if (rw >= rh) {
      const sw = s / rh;
      let cy = ry;
      for (const i of r) {
        const ch = i.a / sw;
        rects.push({ item: i.item, x: rx, y: cy, w: sw, h: ch });
        cy += ch;
      }
      rx += sw;
      rw -= sw;
    } else {
      const sh = s / rw;
      let cx = rx;
      for (const i of r) {
        const cw = i.a / sh;
        rects.push({ item: i.item, x: cx, y: ry, w: cw, h: sh });
        cx += cw;
      }
      ry += sh;
      rh -= sh;
    }
  };
  for (const it of items) {
    const a = { item: it, a: it.size * scale };
    const side = Math.min(rw, rh);
    if (side <= 0) break;
    if (row.length && worst([...row, a], side) > worst(row, side)) {
      layout(row);
      row = [a];
    } else row.push(a);
  }
  if (row.length) layout(row);
  return rects;
}

function childrenOf(d: MapDir): Item[] {
  const items: Item[] = d.kids.filter((k) => k.size > 0).map((k) => ({ block: { kind: "dir", dir: k }, size: k.size }));
  if (d.looseSize > 0) items.push({ block: { kind: "files", dir: d }, size: d.looseSize });
  if (d.more > 0 && d.moreSize > 0) items.push({ block: { kind: "more", dir: d }, size: d.moreSize });
  items.sort((a, b) => b.size - a.size);
  return items;
}

export default function SizeMap({
  load,
  home,
  onShow,
  top = "Everything",
  where,
  showHint = "show in Files",
}: {
  /// The map of a folder by its path, "" for the top. A new function redraws from it.
  load: (path: string) => Promise<MapDir>;
  home: string;
  /// Show a folder (or the folder a block of files is in) in the Files list.
  onShow: (path: string) => void;
  /// The first crumb's name.
  top?: string;
  /// A path as the tooltip shows it.
  where?: (path: string) => string;
  /// What ⌘-click does, for the tooltip.
  showHint?: string;
}) {
  // The folder the map is zoomed into, and the way down to it for the path bar.
  const [at, setAt] = useState<{ path: string; name: string }[]>([{ path: "", name: "" }]);
  const [tree, setTree] = useState<MapDir | null>(null);
  const [err, setErr] = useState("");
  const mapRef = useRef<HTMLDivElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const blocks = useRef(new Map<string, Block>());
  const here = at[at.length - 1].path;

  // A new snapshot keeps the zoom when that folder is still there.
  useEffect(() => {
    let live = true;
    setTree(null);
    setErr("");
    load(here).then(
      (t) => live && setTree(t),
      (e) => {
        if (!live) return;
        if (here) setAt([{ path: "", name: "" }]);
        else setErr(String(e));
      },
    );
    return () => {
      live = false;
    };
  }, [load, here]);

  const label = (name: string) => (name.startsWith("/") ? tilde(name, home) : name);
  const title = (b: Block) =>
    b.kind === "files" ? count(b.dir.looseFiles, "file") : b.kind === "more" ? `${count(b.dir.more, "more folder")}` : label(b.dir.name);
  const sizeOf = (b: Block) => (b.kind === "files" ? b.dir.looseSize : b.kind === "more" ? b.dir.moreSize : b.dir.size);

  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    if (!tree) {
      map.innerHTML = "";
      return;
    }
    const draw = (b: Block, x: number, y: number, w: number, h: number, out: HTMLElement, id: string) => {
      if (w < 3 || h < 3) return;
      blocks.current.set(id, b);
      const d = b.dir;
      const el = document.createElement("div");
      const zoomable = b.kind === "dir" && (d.kids.length > 0 || d.more > 0 || d.looseFiles > 0 || !d.read);
      el.className = b.kind === "dir" ? `blk d${((d.depth - 1) % 8) + 1}${zoomable ? " zoom" : ""}` : `blk ${b.kind}`;
      el.style.cssText = `left:${x}px;top:${y}px;width:${w}px;height:${h}px`;
      el.dataset.id = id;
      let labelled = false;
      if (w >= 56 && h >= 30) {
        labelled = true;
        const hd = document.createElement("div");
        hd.className = "hd";
        const nm = document.createElement("span");
        nm.className = "n";
        nm.textContent = title(b);
        hd.appendChild(nm);
        if (w >= 110) {
          const s = document.createElement("span");
          s.className = "s";
          s.textContent = bytes(sizeOf(b));
          hd.appendChild(s);
        }
        el.appendChild(hd);
      }
      out.appendChild(el);
      if (b.kind !== "dir") return;
      const top = labelled ? HEAD : PAD;
      const iw = w - PAD * 2 - 2,
        ih = h - top - PAD - 2;
      if (iw < 10 || ih < 10) return;
      let i = 0;
      for (const r of squarify(childrenOf(d), iw, ih)) {
        const gx = r.x > 0 ? GAP / 2 : 0,
          gy = r.y > 0 ? GAP / 2 : 0;
        draw(r.item.block, PAD + r.x + gx, top + r.y + gy, r.w - gx - GAP / 2, r.h - gy - GAP / 2, el, `${id}.${i++}`);
      }
    };
    const render = () => {
      blocks.current.clear();
      map.innerHTML = "";
      const W = map.clientWidth,
        H = map.clientHeight;
      if (W < 20 || H < 20) return;
      const items = childrenOf(tree);
      if (!items.length) {
        const e = document.createElement("div");
        e.className = "empty";
        e.textContent = tree.files === 0 ? "This folder is empty." : "Nothing big enough to draw here.";
        map.appendChild(e);
        return;
      }
      let i = 0;
      for (const r of squarify(items, W, H)) {
        const gx = r.x > 0 ? GAP / 2 : 0,
          gy = r.y > 0 ? GAP / 2 : 0;
        const gr = r.x + r.w < W - 0.5 ? GAP / 2 : 0,
          gb = r.y + r.h < H - 0.5 ? GAP / 2 : 0;
        draw(r.item.block, r.x + gx, r.y + gy, r.w - gx - gr, r.h - gy - gb, map, `${i++}`);
      }
    };
    render();
    const ro = new ResizeObserver(render);
    ro.observe(map);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tree, home]);

  const blockAt = (t: EventTarget | null) => {
    const el = (t as HTMLElement | null)?.closest?.(".blk") as HTMLElement | null;
    const b = el && blocks.current.get(el.dataset.id ?? "");
    return el && b ? { el, b } : null;
  };
  const zoom = (d: MapDir) => {
    // The way down from where the map is now, a folder at a time.
    const way: { path: string; name: string }[] = [];
    const find = (x: MapDir): boolean => {
      if (x.path === d.path) return true;
      for (const k of x.kids) {
        way.push({ path: k.path, name: k.name });
        if (find(k)) return true;
        way.pop();
      }
      return false;
    };
    if (tree && find(tree)) setAt([...at, ...way]);
  };
  const onClick = (e: React.MouseEvent) => {
    const hit = blockAt(e.target);
    if (!hit) return;
    const { b, el } = hit;
    if (e.metaKey || b.kind !== "dir") onShow(b.dir.path);
    else if (el.classList.contains("zoom")) zoom(b.dir);
  };
  const onMove = (e: React.MouseEvent) => {
    const tip = tipRef.current;
    if (!tip) return;
    const hit = blockAt(e.target);
    if (!hit) {
      tip.hidden = true;
      return;
    }
    const { b, el } = hit;
    const d = b.dir;
    const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
    const place = esc((where ? where(d.path) : label(d.path)) || "the backup");
    const share = (n: number) => (d.size ? `${Math.round((100 * n) / d.size)}%` : "—");
    let html: string;
    if (b.kind === "files") {
      html =
        `<b>${title(b)} directly in ${esc(label(d.name) || "the backup")}</b><div class="p">${place}</div>` +
        `<div class="r"><span>Size</span><b>${bytes(d.looseSize)}</b><span>Share</span><b>${share(d.looseSize)}</b></div>` +
        `<div class="hint">Click to ${showHint.replace("show", "show them")}</div>`;
    } else if (b.kind === "more") {
      html =
        `<b>${title(b)} in ${esc(label(d.name) || "the backup")}</b><div class="p">${place}</div>` +
        `<div class="r"><span>Size</span><b>${bytes(d.moreSize)}</b><span>Share</span><b>${share(d.moreSize)}</b></div>` +
        `<div class="hint">Each is too small to draw here. Click to ${showHint.replace("show", "see them")}.</div>`;
    } else {
      html =
        `<b>${esc(label(d.name))}</b><div class="p">${place}</div>` +
        `<div class="r"><span>Size</span><b>${bytes(d.size)}</b><span>Files</span><b>${d.files.toLocaleString()}</b></div>` +
        `<div class="hint">${el.classList.contains("zoom") ? "Click to zoom in · " : ""}⌘-click to ${showHint}</div>`;
    }
    tip.innerHTML = html;
    tip.hidden = false;
    const r = tip.getBoundingClientRect();
    let x = e.clientX + 14,
      y = e.clientY + 16;
    if (x + r.width > window.innerWidth - 8) x = e.clientX - r.width - 10;
    if (y + r.height > window.innerHeight - 8) y = e.clientY - r.height - 10;
    tip.style.left = `${x}px`;
    tip.style.top = `${y}px`;
  };

  return (
    <div className="grow col" style={{ minWidth: 0, minHeight: 0 }}>
      <div className="map-bar">
        <button className="btn small" title="Up a folder" disabled={at.length === 1} onClick={() => setAt(at.slice(0, -1))}>
          <Icon name="back" size={13} stroke={2} />
        </button>
        <div className="crumbs">
          {at.map((c, i) => (
            <span key={c.path} className="row" style={{ gap: 4, minWidth: 0 }}>
              {i > 0 && <span className="faint">›</span>}
              <button className={i === at.length - 1 ? "on" : ""} onClick={() => setAt(at.slice(0, i + 1))}>
                {i === 0 ? top : label(c.name)}
              </button>
            </span>
          ))}
        </div>
        <span className="grow" />
        {tree && (
          <span className="small muted nowrap">
            {bytes(tree.size)} · {count(tree.files, "file")}
          </span>
        )}
      </div>
      {err ? (
        <div className="empty">{err}</div>
      ) : (
        <div className="map-wrap">
          {/* Drawn into directly, not by React: thousands of blocks, redrawn on every resize. */}
          <div
            className="map"
            ref={mapRef}
            onClick={onClick}
            onMouseMove={onMove}
            onMouseLeave={() => tipRef.current && (tipRef.current.hidden = true)}
          />
          {!tree && <div className="empty map-wait">Working out what takes the space…</div>}
        </div>
      )}
      <div className="map-tip" ref={tipRef} hidden />
    </div>
  );
}
