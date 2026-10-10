// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The Restore screen's list of files: a snapshot's folders as a tree (or what a search found),
// sortable by column, drawn only where it's in view, with a right-click menu on each row.

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Entry } from "./api";
import { Icon } from "./icons";
import { useMenu } from "./ui";
import { bytes, tilde, when } from "./format";

// On the Mac and in no snapshot this far: nothing to restore or remove.
export const onlyOnMac = (e: Entry) => e.disk === "unsaved" && e.tag !== "deleted";

// The file list draws only the rows in view, so a folder of thousands opens at once. Rows are
// all this tall (.file-row); a few more than fit are drawn either side, for a smooth scroll.
const ROW = 32;
const OVERSCAN = 12;

function iconFor(e: Entry): string {
  if (e.kind === "dir") return "folder";
  if (e.kind === "link") return "link";
  return /\.(jpe?g|png|heic|gif|tiff?|webp|raw|cr2|nef|dng)$/i.test(e.name) ? "image" : "file";
}

/// A row's right-click menu: what the versions pane and the footer do. On one of several chosen
/// items it acts on all of them (`count`), and what only makes sense for one item is greyed out.
export function RowMenu(p: {
  e: Entry;
  count: number;
  inSnapshot: boolean;
  /// A deleted item comes back from the last snapshot that had it, so only one never backed up can't.
  canRestore: boolean;
  chosen: boolean;
  canOpen: boolean;
  isOpen: boolean;
  /// Where the footer restores to: "where it was", or a folder's name; "" when no folder is chosen yet.
  restoreTo: string;
  onMac: boolean;
  quickLook: () => void;
  compare: () => void;
  restore: () => void;
  choose: () => void;
  toggleOpen: () => void;
  reveal: () => void;
  copyPath: () => void;
  remove: () => void;
}) {
  const file = p.e.kind === "file";
  const one = p.count === 1;
  const what = one ? "" : ` ${p.count} items`;
  return (
    <>
      {file && (
        <button role="menuitem" disabled={!one || !p.inSnapshot} onClick={p.quickLook}>
          <Icon name="eye" size={14} stroke={1.9} />
          Quick Look
        </button>
      )}
      {file && (
        <button role="menuitem" disabled={!one || !p.inSnapshot || !p.onMac} onClick={p.compare}>
          <Icon name="compare" size={14} />
          Compare with current
        </button>
      )}
      {p.canOpen && (
        <button role="menuitem" disabled={!one} onClick={p.toggleOpen}>
          <Icon name="forward" size={14} stroke={2.2} />
          {p.isOpen ? "Collapse" : "Expand"}
        </button>
      )}
      <hr />
      <button role="menuitem" disabled={!p.canRestore || !p.restoreTo} onClick={p.restore}>
        <Icon name="down" size={14} />
        {!p.restoreTo
          ? "Restore (choose a folder first)"
          : p.restoreTo === "where it was"
            ? `Restore${what} to where ${one ? "it was" : "they were"}`
            : `Restore${what} into ${p.restoreTo}`}
      </button>
      <button role="menuitem" disabled={!p.canRestore} onClick={p.choose}>
        <Icon name="check" size={14} />
        {p.chosen ? `Unselect${what}` : "Select"}
      </button>
      <hr />
      <button role="menuitem" disabled={!one || !p.onMac} onClick={p.reveal}>
        <Icon name="folder" size={14} />
        Show in Finder
      </button>
      <button role="menuitem" onClick={p.copyPath}>
        <Icon name="link" size={14} />
        {one ? "Copy path" : `Copy ${p.count} paths`}
      </button>
      <hr />
      <button role="menuitem" disabled={!p.canRestore} onClick={p.remove}>
        <Icon name="trash" size={14} />
        {`Remove${what} from backup…`}
      </button>
    </>
  );
}

export type SortBy = "name" | "modified" | "size" | "versions";
export type Sort = { by: SortBy; up: boolean };

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
  sort: Sort;
  setSort: (s: Sort) => void;
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

/** The list, with its heading row. Choosing and opening are the screen's, and so is what the
 *  right-click menu holds (`menu`); the list keeps its scroll, which rows are drawn, and the menu. */
export function FileList(p: {
  kids: Record<string, Entry[]>;
  open: Set<string>;
  /// What a search found, listed flat in place of the tree.
  hits: Entry[] | null;
  sort: Sort;
  setSort: (s: Sort) => void;
  picked: Entry | null;
  setPicked: (e: Entry) => void;
  /// An item to pick and scroll to once its row is listed (shown from the map); `revealed` once it is.
  reveal: { path: string } | null;
  revealed: () => void;
  home: string;
  isChosen: (e: Entry) => boolean;
  partlyChosen: (e: Entry) => boolean;
  toggleCheck: (e: Entry) => void;
  toggleOpen: (path: string) => void;
  /// The heading's box: everything at the top of the list, or nothing.
  allChosen: boolean;
  someChosen: boolean;
  canChooseAll: boolean;
  chooseAll: () => void;
  menu: (e: Entry) => ReactNode;
}) {
  const { kids, open, hits, sort, picked, home } = p;
  // The row a right-click opened the menu for.
  const [menuFor, setMenuFor] = useState<Entry | null>(null);
  const rowMenu = useMenu();

  // Keyed by place in the tree as well as path: a backup made while one source was inside another
  // holds the same path twice. Built again only when what's listed, opened or sorted changes.
  const rows = useMemo(() => {
    const rows: { e: Entry; depth: number; key: string }[] = [];
    const dir = sort.up ? 1 : -1;
    const value = (e: Entry) => (sort.by === "modified" ? e.mtime : sort.by === "size" ? e.size : sort.by === "versions" ? e.versions : 0);
    const sorted = (list: Entry[]) =>
      [...list].sort(
        (a, b) =>
          Number(a.kind !== "dir") - Number(b.kind !== "dir") ||
          dir * (value(a) - value(b)) ||
          dir * a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }),
      );
    const walk = (path: string, depth: number, under: string) => {
      sorted(kids[path] ?? []).forEach((e, i) => {
        // By position: a source's name is its whole path, so names joined up can collide.
        const key = `${under}.${i}`;
        rows.push({ e, depth, key });
        if (e.kind === "dir" && open.has(e.path) && e.tag !== "deleted" && !onlyOnMac(e)) walk(e.path, depth + 1, key);
      });
    };
    if (hits) sorted(hits).forEach((e, i) => rows.push({ e, depth: 0, key: `${i}` }));
    else walk("", 0, "");
    return rows;
  }, [kids, open, sort, hits]);

  // The list's scroll, as the first row in view, and its height: which rows to draw.
  // The list comes and goes with the view and the snapshot, so it's watched from when it appears.
  const scroller = useRef<HTMLDivElement | null>(null);
  const [first, setFirst] = useState(0);
  const [tall, setTall] = useState(0);
  const watch = useRef<ResizeObserver | null>(null);
  const listRef = useCallback((el: HTMLDivElement | null) => {
    scroller.current = el;
    watch.current?.disconnect();
    watch.current = null;
    if (!el) return;
    watch.current = new ResizeObserver(() => setTall(el.clientHeight));
    watch.current.observe(el);
    setFirst(Math.floor(el.scrollTop / ROW));
  }, []);
  // Kept within the list when it gets shorter (a search, a folder closed), so the list shrinks,
  // the scroll comes back inside it and onScroll catches up, rather than showing a blank page.
  const from = Math.max(0, Math.min(first, rows.length - Math.ceil(tall / ROW)) - OVERSCAN);
  const to = Math.min(rows.length, first + Math.ceil(tall / ROW) + OVERSCAN);

  // Shown from the map: pick it once the folders down to it are listed, and scroll to it by its
  // place in the list, since its row isn't drawn until then.
  const { reveal, setPicked, revealed } = p;
  useEffect(() => {
    const i = reveal ? rows.findIndex((r) => r.e.path === reveal.path) : -1;
    if (i < 0) return;
    setPicked(rows[i].e);
    revealed();
    const el = scroller.current;
    if (el) el.scrollTop = i * ROW - (el.clientHeight - ROW) / 2;
    // The screen's callbacks are new each render; only a new item or new rows look again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reveal, rows]);

  return (
    <div className="grow" style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
      <div className="files-head caps" style={{ letterSpacing: "0.04em" }}>
        <input
          type="checkbox"
          className="pick"
          aria-label={p.allChosen ? "Select none" : "Select all"}
          title={p.allChosen ? "Select none" : "Select all"}
          checked={p.allChosen}
          ref={(el) => {
            if (el) el.indeterminate = p.someChosen;
          }}
          disabled={!p.canChooseAll}
          onChange={p.chooseAll}
        />
        <SortHead by="name" sort={sort} setSort={p.setSort} className="grow" style={{ paddingLeft: 40 }}>
          {hits ? `${hits.length} found` : "Name"}
        </SortHead>
        <SortHead by="modified" sort={sort} setSort={p.setSort} style={{ width: 140 }}>
          Modified
        </SortHead>
        <SortHead by="size" sort={sort} setSort={p.setSort} style={{ width: 76, justifyContent: "flex-end" }}>
          Size
        </SortHead>
        <SortHead by="versions" sort={sort} setSort={p.setSort} style={{ width: 70, justifyContent: "flex-end" }}>
          Versions
        </SortHead>
        <span style={{ width: 96 }} />
      </div>
      <div
        ref={listRef}
        style={{ flexGrow: 1, overflow: "auto" }}
        onScroll={(ev) => setFirst(Math.floor(ev.currentTarget.scrollTop / ROW))}
      >
        <div style={{ height: from * ROW }} />
        {rows.slice(from, to).map(({ e, depth, key }) => {
          const name = e.name.startsWith("/") ? tilde(e.name, home) : hits ? tilde(e.path, home) : e.name;
          return (
            <div
              key={key}
              className={`file-row${picked?.path === e.path ? " sel" : ""}${e.tag === "deleted" ? " gone" : ""}`}
              onContextMenu={(ev) => {
                setPicked(e);
                setMenuFor(e);
                rowMenu.openAt(ev);
              }}
            >
              <input
                type="checkbox"
                className="pick"
                aria-label={`Select ${e.name}`}
                style={{ marginLeft: depth * 20 }}
                checked={p.isChosen(e)}
                ref={(el) => {
                  if (el) el.indeterminate = p.partlyChosen(e);
                }}
                disabled={onlyOnMac(e)}
                onChange={() => p.toggleCheck(e)}
              />
              {/* Keeps the gap the indent had, so names stay where they were. */}
              <span style={{ width: 0, flexShrink: 0 }} />
              {e.kind === "dir" && !hits && e.tag !== "deleted" && !onlyOnMac(e) ? (
                <button
                  className={`disc${open.has(e.path) ? " open" : ""}`}
                  aria-label={`${open.has(e.path) ? "Collapse" : "Expand"} ${e.name}`}
                  onClick={() => p.toggleOpen(e.path)}
                >
                  <Icon name="forward" size={12} stroke={2.6} />
                </button>
              ) : (
                <span style={{ width: 16, flexShrink: 0 }} />
              )}
              <button className="name" onClick={() => setPicked(e)} onDoubleClick={() => e.kind === "dir" && p.toggleOpen(e.path)}>
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
                <span style={{ width: 96, display: "flex", justifyContent: "flex-end" }}>
                  {e.disk === "gone" ? (
                    <span className="tag deleted" title="In this backup, but no longer on your Mac">
                      Not on Mac
                    </span>
                  ) : e.disk === "unsaved" ? (
                    <span className="tag plain" title="On your Mac, but not in the backup yet">
                      Not backed up
                    </span>
                  ) : (
                    e.tag && <span className={`tag ${e.tag}`}>{e.tag === "new" ? "New" : e.tag === "changed" ? "Changed" : "Deleted"}</span>
                  )}
                </span>
              </button>
            </div>
          );
        })}
        <div style={{ height: (rows.length - to) * ROW }} />
        {rows.length === 0 && <div className="empty">{hits ? "Nothing by that name in this snapshot." : "Loading…"}</div>}
        {menuFor && <rowMenu.Menu width={240}>{p.menu(menuFor)}</rowMenu.Menu>}
      </div>
    </div>
  );
}
