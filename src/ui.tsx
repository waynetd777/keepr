// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Shared controls: the app's own tooltip (never macOS's), switches, segmented controls, sheets,
// pop-up menus and a toast for passing messages.

import { api, type JobStatus, type Queued } from "./api";
import { Icon } from "./icons";
import { secondsLeft } from "./format";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";

/** Every tooltip in the app. Give an element a `title` (or an icon-only button an aria-label). */
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
      const text = t.dataset.tip || (!t.textContent?.trim() ? t.getAttribute("aria-label") : null);
      if (!text) return;
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

const ToastContext = createContext<(msg: string) => void>(() => {});

export function ToastProvider({ children }: { children: ReactNode }) {
  const [msg, setMsg] = useState<string | null>(null);
  const timer = useRef<number>(undefined);
  const show = useCallback((m: string) => {
    setMsg(m);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMsg(null), 5000);
  }, []);
  return (
    <ToastContext.Provider value={show}>
      {children}
      {msg && (
        <div className="toast" role="status" onClick={() => setMsg(null)}>
          {msg}
        </div>
      )}
    </ToastContext.Provider>
  );
}

export const useToast = () => useContext(ToastContext);

/** Runs an action and shows its error, if any, as a toast. */
export function useAct() {
  const toast = useToast();
  return useCallback(
    async <T,>(f: () => Promise<T>): Promise<T | undefined> => {
      try {
        return await f();
      } catch (e) {
        toast(String(e));
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
      className={`btn${small ? " small" : ""}`}
      style={iconOnly ? { width: 30, padding: 0 } : undefined}
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

/** When a server is chosen, fill in a login already saved for it (Keepr's or Finder's) and list
 *  its shares. `password` shows SAVED_PASSWORD until someone types a new one. */
export function useSavedLogin(
  server: string,
  user: string,
  setUser: (u: string) => void,
  setPassword: (p: string) => void,
  onShares: (s: string[]) => void,
  onMessage: (m: string) => void,
) {
  const [source, setSource] = useState<"keepr" | "finder" | null>(null);
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
// await an answer. One at a time: a second waits for the first.
let asking: { q: Ask; answer: (yes: boolean) => void } | null = null;
const waiting: { q: Ask; answer: (yes: boolean) => void }[] = [];
const askSubs = new Set<() => void>();
const askChanged = () => askSubs.forEach((f) => f());

/** Asks in the app's own dialog, in place of the system's; true when the user goes ahead. */
export function confirm(q: Ask): Promise<boolean> {
  return new Promise((answer) => {
    waiting.push({ q, answer });
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
  const { q, answer } = asking;
  const done = (yes: boolean) => {
    answer(yes);
    asking = waiting.shift() ?? null;
    askChanged();
  };
  return (
    <Sheet
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
  return { run, stop };
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
