// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Shared controls: the app's own tooltip (never the system's), switches, segmented controls,
// sheets, pop-up menus and a toast for passing messages.

import { api, type JobStatus, type Queued } from "./api";
import { isMac, shortcut } from "./platform";
import { Icon } from "./icons";
import { secondsLeft } from "./format";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

/** Every tooltip in the app. Give an element a `title` (or an icon-only button an aria-label).
 *  Shortcuts in it are written as macOS does ("⌘K"), and read as Windows' ("Ctrl+K") there. */
export function Tooltips() {
  const [tip, setTip] = useState<{ text: string; x: number; y: number; side: "below" | "above" | "right" } | null>(null);
  useEffect(() => {
    let timer: number | undefined;
    let el: HTMLElement | null = null;
    const hide = () => {
      window.clearTimeout(timer);
      el = null;
      setTip(null);
    };
    const over = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const t = (target.closest?.("[title], [data-tip]") ?? target.closest?.("button[aria-label]")) as HTMLElement | null;
      if (t === el) return;
      hide();
      if (!t) return;
      if (t.title) {
        t.dataset.tip = t.title;
        t.removeAttribute("title");
      }
      const raw = t.dataset.tip || (!t.textContent?.trim() ? t.getAttribute("aria-label") : null);
      if (!raw) return;
      const text = shortcut(raw);
      el = t;
      timer = window.setTimeout(() => {
        if (el !== t || !t.isConnected || document.documentElement.dataset.scene) return;
        const r = t.getBoundingClientRect();
        const x = Math.max(150, Math.min(r.left + r.width / 2, window.innerWidth - 150));
        if (t.closest(".sidebar")) setTip({ text, side: "right", x: r.right + 8, y: r.top + r.height / 2 });
        else if (r.bottom + 44 > window.innerHeight) setTip({ text, side: "above", x, y: r.top - 6 });
        else setTip({ text, side: "below", x, y: r.bottom + 6 });
      }, 450);
    };
    document.addEventListener("mouseover", over);
    document.addEventListener("mousedown", hide, true);
    document.addEventListener("scroll", hide, true);
    window.addEventListener("blur", hide);
    return () => {
      hide();
      document.removeEventListener("mouseover", over);
      document.removeEventListener("mousedown", hide, true);
      document.removeEventListener("scroll", hide, true);
      window.removeEventListener("blur", hide);
    };
  }, []);
  if (!tip) return null;
  const style: React.CSSProperties =
    tip.side === "right"
      ? { left: tip.x, top: tip.y, transform: "translateY(-50%)" }
      : { left: tip.x, top: tip.y, transform: tip.side === "above" ? "translate(-50%, -100%)" : "translateX(-50%)" };
  return (
    <div className="tip" role="tooltip" style={style}>
      {tip.text}
    </div>
  );
}

export function Switch({
  on,
  onChange,
  label,
  disabled,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      className={`switch${on ? " on" : ""}`}
      onClick={() => onChange(!on)}
    />
  );
}

export function Seg<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: [T, string][];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map(([v, text]) => (
        <button key={v} type="button" aria-pressed={v === value} className={v === value ? "on" : ""} onClick={() => onChange(v)}>
          {text}
        </button>
      ))}
    </div>
  );
}

export function Sheet({
  title,
  subtitle,
  icon,
  width = 620,
  onClose,
  children,
  foot,
}: {
  title: string;
  subtitle?: ReactNode;
  /** Beside the title, as the update dialog's mark is. */
  icon?: ReactNode;
  width?: number;
  onClose: () => void;
  children: ReactNode;
  foot?: ReactNode;
}) {
  const scrim = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Esc closes the sheet on top only: a question asked over a sheet goes first.
    const k = (e: KeyboardEvent) => {
      const all = document.querySelectorAll(".scrim");
      if (e.key === "Escape" && all[all.length - 1] === scrim.current) onClose();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  return (
    <div className="scrim" ref={scrim} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <section role="dialog" aria-modal="true" aria-label={title} className="sheet" style={{ width }}>
        <div className={icon ? "sheet-head with-icon" : "sheet-head"}>
          {icon}
          <div className="sheet-title">
            <h2>{title}</h2>
            {subtitle && <span className="muted">{subtitle}</span>}
          </div>
        </div>
        {children}
        {foot && <div className="sheet-foot">{foot}</div>}
      </section>
    </div>
  );
}

/** A pop-up menu under the element that opened it, or at the pointer for a right-click (openAt);
 *  closes on a click elsewhere or Escape. */
export function useMenu() {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!at) return;
    const down = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setAt(null);
    const key = (e: KeyboardEvent) => e.key === "Escape" && setAt(null);
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key);
    };
  }, [at]);
  const open = (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setAt(at ? null : { x: r.left, y: r.bottom + 4 });
  };
  const openAt = (e: React.MouseEvent) => {
    e.preventDefault();
    setAt({ x: e.clientX, y: e.clientY });
  };
  // The same component from one render to the next while the menu stays where it is, so a parent
  // re-rendering (a job event every second) does not remount the menu and what is in it.
  const Menu = useCallback(
    ({ children, width = 260 }: { children: ReactNode; width?: number }) =>
      at ? (
        <MenuPanel at={at} width={width} panelRef={ref} close={() => setAt(null)}>
          {children}
        </MenuPanel>
      ) : null,
    [at],
  );
  return { open, openAt, close: () => setAt(null), Menu, isOpen: !!at };
}

type SelectItem = { value: string; label: string; disabled: boolean; group: string | null };

/** Every select in the window opens Keepr's own menu on Windows, where Edge's engine draws the
 *  system's list (square, in the system font, with a grey highlight). The select itself stays,
 *  closed, as each screen styles it, and keeps the keyboard's up and down; a click, Space, Enter,
 *  Alt+Down or F4 opens the menu in its place. The menu lists the select's own options (and
 *  groups), and picking one sets the select's value and sends the change React listens for, so
 *  the screens' selects need nothing of their own. macOS keeps its native menus. Mounted once per
 *  window. */
export function SelectMenus() {
  const [open, setOpen] = useState<{ el: HTMLSelectElement; items: SelectItem[]; active: number; rect: DOMRect } | null>(null);
  const panel = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });
  // The select whose menu is open, for the click handler set up once below.
  const openFor = useRef<HTMLSelectElement | null>(null);
  useEffect(() => {
    openFor.current = open?.el ?? null;
  }, [open]);
  useEffect(() => {
    if (isMac) return;
    const items = (el: HTMLSelectElement): SelectItem[] =>
      [...el.options].map((o) => ({
        value: o.value,
        label: o.label || o.text,
        disabled: o.disabled,
        group: o.parentElement instanceof HTMLOptGroupElement ? o.parentElement.label : null,
      }));
    const show = (el: HTMLSelectElement) => {
      if (el.disabled) return;
      const list = items(el);
      el.focus();
      setOpen({
        el,
        items: list,
        active: Math.max(
          0,
          list.findIndex((i) => i.value === el.value),
        ),
        rect: el.getBoundingClientRect(),
      });
    };
    const down = (e: MouseEvent) => {
      const el = (e.target as HTMLElement).closest?.("select");
      if (!(el instanceof HTMLSelectElement) || e.button !== 0) return;
      e.preventDefault();
      // A second click on the same select closes its menu; a click on another opens that one's.
      if (openFor.current === el) setOpen(null);
      else show(el);
    };
    const key = (e: KeyboardEvent) => {
      const el = e.target;
      if (!(el instanceof HTMLSelectElement) || openFor.current) return;
      if (e.key === " " || e.key === "Enter" || e.key === "F4" || (e.altKey && e.key === "ArrowDown")) {
        e.preventDefault();
        show(el);
      }
    };
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key, true);
    };
  }, []);
  const pick = useCallback(
    (i: number) => {
      const item = open?.items[i];
      if (!open || !item || item.disabled) return;
      if (item.value !== open.el.value) {
        // Through the element's own setter, so React sees a change it didn't make.
        Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set?.call(open.el, item.value);
        open.el.dispatchEvent(new Event("change", { bubbles: true }));
      }
      setOpen(null);
      open.el.focus();
    },
    [open],
  );
  useEffect(() => {
    if (!open) return;
    const step = (from: number, by: number) => {
      for (let i = from + by; i >= 0 && i < open.items.length; i += by) if (!open.items[i].disabled) return i;
      return from;
    };
    const key = (e: KeyboardEvent) => {
      const k = e.key;
      if (k === "Escape" || k === "Tab") {
        if (k === "Escape") e.preventDefault();
        setOpen(null);
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      if (k === "ArrowDown") setOpen((o) => o && { ...o, active: step(o.active, 1) });
      else if (k === "ArrowUp") setOpen((o) => o && { ...o, active: step(o.active, -1) });
      else if (k === "Home") setOpen((o) => o && { ...o, active: step(-1, 1) });
      else if (k === "End") setOpen((o) => o && { ...o, active: step(o.items.length, -1) });
      else if (k === "Enter" || k === " ") pick(open.active);
      else if (k.length === 1) {
        // Typing jumps to the first item that starts with what's been typed in the last second.
        const t = typed.current;
        t.text = (Date.now() - t.at < 1000 ? t.text : "") + k.toLowerCase();
        t.at = Date.now();
        const i = open.items.findIndex((it) => !it.disabled && it.label.toLowerCase().startsWith(t.text));
        if (i >= 0) setOpen((o) => o && { ...o, active: i });
      }
    };
    const away = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (!panel.current?.contains(t) && !t.closest?.("select")) setOpen(null);
    };
    const gone = () => setOpen(null);
    // Scrolling the page moves the select from under its menu; scrolling the menu doesn't.
    const scrolled = (e: Event) => !panel.current?.contains(e.target as Node) && setOpen(null);
    window.addEventListener("keydown", key, true);
    window.addEventListener("mousedown", away, true);
    window.addEventListener("resize", gone);
    window.addEventListener("scroll", scrolled, true);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("resize", gone);
      window.removeEventListener("scroll", scrolled, true);
    };
  }, [open, pick]);
  // The chosen item in view, as the list opens and as the keys move.
  useEffect(() => {
    panel.current?.querySelector(".active")?.scrollIntoView({ block: "nearest" });
  }, [open?.active]);
  if (!open) return null;
  const { rect, items, active, el } = open;
  const width = Math.max(rect.width, 180);
  const below = window.innerHeight - rect.bottom - 12;
  const above = rect.top - 12;
  const want = Math.min(items.length * 33 + 14 + new Set(items.map((i) => i.group).filter(Boolean)).size * 30, 360);
  const up = below < want && above > below;
  const maxHeight = Math.min(want, up ? above : below);
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
  return (
    <div
      ref={panel}
      className="menu select-menu"
      role="listbox"
      aria-label={el.getAttribute("aria-label") ?? undefined}
      style={{
        position: "fixed",
        left,
        width,
        maxHeight,
        overflowY: "auto",
        ...(up ? { bottom: window.innerHeight - rect.top + 4 } : { top: rect.bottom + 4 }),
      }}
    >
      {items.map((it, i) => {
        const head = it.group && (i === 0 || items[i - 1].group !== it.group) ? it.group : null;
        return (
          <div key={`${it.group}:${it.value}`} style={{ display: "contents" }}>
            {head && <div className="select-group">{head}</div>}
            <button
              type="button"
              role="option"
              aria-selected={it.value === el.value}
              disabled={it.disabled}
              className={i === active ? "active" : ""}
              onMouseEnter={() => setOpen((o) => o && { ...o, active: i })}
              onClick={() => pick(i)}
            >
              <span className="grow ellipsis">{it.label}</span>
              {it.value === el.value && <Icon name="check" size={13} stroke={2.4} />}
            </button>
          </div>
        );
      })}
    </div>
  );
}

/** useMenu's menu itself, at a point on the window. */
function MenuPanel({
  at,
  width,
  panelRef,
  close,
  children,
}: {
  at: { x: number; y: number };
  width: number;
  panelRef: React.RefObject<HTMLDivElement | null>;
  close: () => void;
  children: ReactNode;
}) {
  return (
    <div
      ref={(el) => {
        panelRef.current = el;
        // Near the bottom of the window, open upwards instead.
        if (el && el.getBoundingClientRect().bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, at.y - el.offsetHeight)}px`;
      }}
      className="menu"
      role="menu"
      style={{ position: "fixed", left: Math.min(at.x, window.innerWidth - width - 12), top: at.y, width }}
      onClick={close}
    >
      {children}
    </div>
  );
}

const ToastContext = createContext<(msg: string, kind?: "error") => void>(() => {});
const DismissContext = createContext<() => void>(() => {});

/** A note that goes after a few seconds, and an error that stays until it's closed (or the thing
 * that failed is done after all), so a failure can't pass unseen. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number>(undefined);
  const show = useCallback((m: string, kind?: "error") => {
    if (kind === "error") {
      setError(m);
      return;
    }
    setMsg(m);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMsg(null), 5000);
  }, []);
  const dismiss = useCallback(() => setError(null), []);
  return (
    <ToastContext.Provider value={show}>
      <DismissContext.Provider value={dismiss}>
        {children}
        {(msg || error) && (
          <div className="toasts">
            {error && (
              <div className="toast error" role="alert">
                <Icon name="warning" size={16} />
                <span className="grow">{error}</span>
                <button className="toast-close" aria-label="Close" title="Close" onClick={dismiss}>
                  <Icon name="close" size={12} stroke={2.4} />
                </button>
              </div>
            )}
            {msg && (
              <div className="toast" role="status" onClick={() => setMsg(null)}>
                {msg}
              </div>
            )}
          </div>
        )}
      </DismissContext.Provider>
    </ToastContext.Provider>
  );
}

export const useToast = () => useContext(ToastContext);
/** Closes the error toast: what failed has since been done. */
export const useDismissError = () => useContext(DismissContext);

/** Runs an action and shows its error, if any, as a toast. */
export function useAct() {
  const toast = useToast();
  return useCallback(
    async <T,>(f: () => Promise<T>): Promise<T | undefined> => {
      try {
        return await f();
      } catch (e) {
        toast(String(e), "error");
        return undefined;
      }
    },
    [toast],
  );
}

// A plan's own commands, run before and after the backup proper (core.rs names these stages).
// Nothing is read while they run, so they're shown by name rather than as progress.
export const BEFORE_COMMAND = "Running the command before the backup";
export const AFTER_COMMAND = "Running the command after the backup";

/** What a running job is doing, for a headline: "Backing up Photos", "Restoring from Photos"… */
export function jobTitle(job: JobStatus): string {
  const verb =
    job.kind === "restore"
      ? "Restoring from"
      : job.kind === "check"
        ? "Checking"
        : job.kind === "prune"
          ? "Tidying up"
          : job.kind === "remove"
            ? "Removing from"
            : job.kind === "delete"
              ? "Deleting the backup of"
              : "Backing up";
  return `${verb} ${job.planName}`;
}

export function pct(job: JobStatus): number {
  if (job.bytesToRead > 0) return Math.min(100, (100 * job.bytesRead) / job.bytesToRead);
  return job.stage === "Saving" ? 99 : 0;
}

/** A plan's running or waiting job: a bar, what it's doing, and Stop (or Cancel while it waits). */
export function PlanProgress({ job, queued, compact }: { job?: JobStatus | null; queued?: Queued; compact?: boolean }) {
  if (!job && !queued) return null;
  if (!job && queued) {
    return (
      <div className="row small muted" style={{ gap: 10 }}>
        <span className="dot grey" />
        <span className="grow">Waiting to start{queued.kind === "full" ? " (full backup)" : ""}</span>
        <button className="btn small" onClick={() => api.cancel(queued.id)}>
          Cancel
        </button>
      </div>
    );
  }
  const j = job!;
  const p = pct(j);
  const what =
    j.kind === "check"
      ? "Checking"
      : j.kind === "prune"
        ? "Tidying up"
        : j.kind === "delete"
          ? "Deleting its backup"
          : j.kind === "remove"
            ? "Removing a folder's data"
            : j.kind === "restore"
              ? "Restoring"
              : j.stage || "Starting";
  return (
    <div className="col" style={{ gap: 6 }}>
      <div className="row" style={{ gap: 10 }}>
        <span className="small" style={{ fontWeight: 600, color: "var(--accent-text)" }}>
          {Math.floor(p)}%
        </span>
        <span className="small muted grow ellipsis">
          {j.stopping ? STOPPING : j.paused ? "Paused" : what}
          {!compact && !j.stopping && j.etaSecs != null && !j.paused ? ` · ${secondsLeft(j.etaSecs)}` : ""}
        </span>
        <button className="btn small" disabled={j.stopping} onClick={() => api.pause(!j.paused)} title={j.paused ? "Resume" : "Pause"}>
          <Icon name={j.paused ? "play" : "pause"} size={12} stroke={2.4} />
          {j.paused ? "Resume" : "Pause"}
        </button>
        <StopButton job={j} small />
      </div>
      <div className="progress" style={{ height: 6 }}>
        <div className="solid" style={{ width: `${p}%`, transition: "width 0.4s" }} />
      </div>
    </div>
  );
}

export const STOPPING = "Stopping: finishing the current file…";

/** Stop for a running job. Greys out and says Stopping… from the click until the job has ended,
 *  because it only stops at a safe point: after the chunk or file it is on. */
export function StopButton({
  job,
  small,
  iconOnly,
  label = "Stop",
}: {
  job: JobStatus;
  small?: boolean;
  iconOnly?: boolean;
  label?: string;
}) {
  const [asked, setAsked] = useState<string | null>(null);
  const stopping = job.stopping || asked === job.id;
  const title = stopping ? "Finishing the current file, then stopping" : "Stop. Nothing half-done is kept as a snapshot.";
  return (
    <button
      className={`btn${small ? " small" : ""}${iconOnly ? " square" : ""}`}
      disabled={stopping}
      aria-label={iconOnly ? (stopping ? "Stopping" : label) : undefined}
      title={title}
      onClick={() => {
        setAsked(job.id);
        api.cancel(job.id);
      }}
    >
      {stopping ? <span className="dot spin" style={{ width: 10, height: 10 }} /> : <Icon name="stop" size={12} />}
      {!iconOnly && (stopping ? "Stopping…" : label)}
    </button>
  );
}

const DEST_ICONS: Record<string, string> = { folder: "folder", drive: "drive", cloud: "cloud", smb: "server", s3: "bucket" };

/** A destination's icon, with a tooltip saying what it is ("OneDrive Personal", "SMB share"). */
export function DestIcon({ kind, label, size = 20 }: { kind?: string; label?: string; size?: number }) {
  return (
    <span title={label} style={{ display: "inline-flex", flexShrink: 0 }}>
      <Icon name={DEST_ICONS[kind ?? ""] ?? "drive"} size={size} />
    </span>
  );
}

/** The x that empties a search box: a 22px round button, easy to hit. */
export function ClearButton({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="clear-btn" aria-label="Clear the search" title="Clear" onClick={onClick}>
      <Icon name="close" size={12} stroke={2.6} />
    </button>
  );
}

/** Stands in for a saved password in a password field: never sent, the saved one is used. */
export const SAVED_PASSWORD = "••••••••";

/** When a server is chosen, fill in a login already saved for it (Keepr's, or Finder's on macOS and
 *  Windows' own on Windows) and list
 *  its shares. `password` shows SAVED_PASSWORD until someone types a new one. */
export function useSavedLogin(
  server: string,
  user: string,
  setUser: (u: string) => void,
  setPassword: (p: string) => void,
  onShares: (s: string[]) => void,
  onMessage: (m: string) => void,
) {
  const [source, setSource] = useState<"keepr" | "finder" | "windows" | null>(null);
  useEffect(() => {
    const host = server.trim();
    if (!host) return;
    // A reply for a server since changed is dropped, so the old server's login and shares never
    // fill in the form for the new one.
    let live = true;
    const t = window.setTimeout(async () => {
      const saved = await api.savedSmbLogin(host).catch(() => null);
      if (!live) return;
      if (!saved || (user && user !== saved.user)) return setSource(null);
      setUser(saved.user);
      setPassword(SAVED_PASSWORD);
      setSource(saved.source);
      onMessage("Asking for its shares…");
      try {
        const shares = await api.listShares(host, saved.user);
        if (!live) return;
        onShares(shares);
        onMessage("");
      } catch (e) {
        if (live) onMessage(String(e));
      }
    }, 450);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
    // Only a new server looks again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [server]);
  return source;
}

/** What `confirm` asks: a title, the question, and the button that goes ahead. */
export type Ask = {
  title: string;
  text: string;
  /** The button that goes ahead: "Delete", "Turn Off". */
  ok: string;
  /** It deletes or can't be undone: the button is red. */
  danger?: boolean;
  /** No Cancel, for a message that only needs reading ("OK"). */
  only?: boolean;
};

// The question on screen, if any, and how it's answered; kept outside React so any handler can
// await an answer. One at a time: a second waits for the first. Each has its own `n`, so the next
// is a new dialog, whose button takes the focus, not the last one's left where it was.
let asking: { q: Ask; answer: (yes: boolean) => void; n: number } | null = null;
const waiting: { q: Ask; answer: (yes: boolean) => void; n: number }[] = [];
let asked = 0;
const askSubs = new Set<() => void>();
const askChanged = () => askSubs.forEach((f) => f());

/** Asks in the app's own dialog, in place of the system's; true when the user goes ahead. */
export function confirm(q: Ask): Promise<boolean> {
  return new Promise((answer) => {
    waiting.push({ q, answer, n: ++asked });
    if (!asking) {
      asking = waiting.shift() ?? null;
      askChanged();
    }
  });
}

/** Says something in the app's own dialog, with OK. */
export const tell = (title: string, text: string) => confirm({ title, text, ok: "OK", only: true }).then(() => {});

/** Shows the question `confirm` asks; mounted once, in App. */
export function Confirm() {
  const [, redraw] = useState(0);
  useEffect(() => {
    const f = () => redraw((n) => n + 1);
    askSubs.add(f);
    return () => void askSubs.delete(f);
  }, []);
  if (!asking) return null;
  const { q, answer, n } = asking;
  const done = (yes: boolean) => {
    answer(yes);
    asking = waiting.shift() ?? null;
    askChanged();
  };
  return (
    <Sheet
      key={n}
      title={q.title}
      icon={
        q.danger ? (
          <span style={{ color: "var(--amber)", display: "flex", flexShrink: 0 }}>
            <Icon name="warning" size={30} />
          </span>
        ) : undefined
      }
      width={460}
      onClose={() => done(false)}
      foot={
        <>
          {!q.only && (
            <button className="btn" autoFocus={q.danger} title="Leave it as it is" onClick={() => done(false)}>
              Cancel
            </button>
          )}
          <button
            className={`btn ${q.danger ? "destroy" : "primary"}`}
            autoFocus={!q.danger}
            title={q.only ? "Close" : q.ok}
            onClick={() => done(true)}
          >
            {q.ok}
          </button>
        </>
      }
    >
      <div className="sheet-body">
        <p style={{ margin: 0, lineHeight: 1.5, userSelect: "text" }}>{q.text}</p>
      </div>
    </Sheet>
  );
}

let tokens = 0;
/** What a request stopped with Cancel answers (src-tauri/src/cancel.rs); not an error to show. */
export const STOPPED = "Stopped.";
export const stopped = (e: unknown) => String(e) === STOPPED;

/**
 * A screen's requests to the backend that it can stop: `run(f)` gives `f` a token for its call
 * and stops it when the screen goes; with a `slot`, a new request in the slot stops the one before
 * (a new search stops the last). `stop(slot)` stops one slot's, `stop()` all of them.
 */
export function useRequests() {
  const live = useRef(new Map<string, string | undefined>());
  useEffect(() => {
    const m = live.current;
    return () => m.forEach((_, t) => void api.stopRequest(t));
  }, []);
  const stop = useCallback((slot?: string) => {
    for (const [t, s] of live.current)
      if (slot === undefined || s === slot) {
        live.current.delete(t);
        void api.stopRequest(t);
      }
  }, []);
  const run = useCallback(
    <T,>(f: (token: string) => Promise<T>, slot?: string): Promise<T> => {
      if (slot !== undefined) stop(slot);
      const t = `r${Date.now().toString(36)}-${(tokens++).toString(36)}`;
      live.current.set(t, slot);
      return f(t).finally(() => live.current.delete(t));
    },
    [stop],
  );
  // The same object every render, so effects that use it don't run again on each one.
  return useMemo(() => ({ run, stop }), [run, stop]);
}

/** Shows `msg` as a toast if `p` takes longer than half a second, for a wait with nothing else to show. */
export function useSlowToast() {
  const toast = useToast();
  return useCallback(
    <T,>(p: Promise<T>, msg: string): Promise<T> => {
      const t = window.setTimeout(() => toast(msg), 500);
      return p.finally(() => window.clearTimeout(t));
    },
    [toast],
  );
}
