// The window: sidebar, toolbar, one history of places for back and forward, and the screens.

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { api, on, type Config, type JobStatus, type Overview as OverviewData } from "./api";
import { Icon, Mark } from "./icons";
import { pct, ToastProvider, Tooltips } from "./ui";
import { bytes } from "./format";
import Overview from "./Overview";
import Plans from "./Plans";
import Restore from "./Restore";
import Activity from "./Activity";
import Destinations from "./Destinations";
import Settings from "./Settings";
import { hideSplash } from "./splash";

export type Screen =
  | { name: "overview" }
  | { name: "plans"; plan?: string; isNew?: boolean }
  | { name: "restore"; plan?: string; query?: string }
  | { name: "activity" }
  | { name: "destinations"; add?: boolean }
  | { name: "settings" };

type AppCtx = {
  ov: OverviewData | null;
  cfg: Config | null;
  job: JobStatus | null;
  home: string;
  screen: Screen;
  go: (s: Screen) => void;
  refresh: () => Promise<void>;
};

const Ctx = createContext<AppCtx>(null as unknown as AppCtx);
export const useApp = () => useContext(Ctx);

// Plans aren't here: they're listed under Backup Plans below, each opening its own settings.
const NAV: [Screen["name"], string, string, string][] = [
  ["overview", "Overview", "overview", "⌘1"],
  ["restore", "Restore", "restore", "⌘2"],
  ["activity", "Activity", "activity", "⌘3"],
  ["destinations", "Destinations", "server", "⌘4"],
];

export function statusDot(status: string): string {
  if (status === "running") return "dot spin";
  if (status === "waiting" || status === "stale") return "dot amber";
  if (status === "failed") return "dot red";
  if (status === "off" || status === "never") return "dot grey";
  return "dot";
}

function Sidebar() {
  const { ov, screen, go } = useApp();
  const restoring = screen.name === "restore";
  const plans = ov?.plans ?? [];
  const nas = ov?.destinations.find((d) => d.total);
  return (
    <nav className="sidebar" aria-label="Sidebar">
      <div className="brand">
        <Mark size={28} />
        Keepr
      </div>
      <div className="nav">
        {NAV.map(([name, label, icon, key]) => (
          <button key={name} className={screen.name === name ? "on" : ""} onClick={() => go({ name } as Screen)} title={`${label} ${key}`}>
            <Icon name={icon} />
            <span className="grow">{label}</span>
            <span className="key">{key}</span>
          </button>
        ))}
      </div>
      <div className="side-section">
        <span className="caps">{restoring ? "Restore from" : "Backup Plans"}</span>
      </div>
      <div className="col" style={{ gap: 2 }}>
        {plans.map((p) => {
          const on = (screen.name === "restore" || screen.name === "plans") && (screen.plan === p.id || (!screen.plan && !(screen.name === "plans" && screen.isNew) && plans[0].id === p.id));
          return (
            <button key={p.id} className={`side-item${on ? " on" : ""}`} onClick={() => go(restoring ? { name: "restore", plan: p.id } : { name: "plans", plan: p.id })} title={restoring ? `Restore from ${p.name}` : `${p.name}: ${p.schedule}`}>
              <span className={statusDot(p.status)} />
              <span className="grow ellipsis">{p.name}</span>
              <span className="tiny faint">{restoring ? p.snapshots.toLocaleString() : p.status === "running" ? "…" : shortAgo(p.lastSuccess)}</span>
            </button>
          );
        })}
        {!restoring && (
          <button className={`side-item${screen.name === "plans" && screen.isNew ? " on" : ""}`} onClick={() => go({ name: "plans", isNew: true })} title="Make a new backup plan ⌘N" style={{ color: "var(--accent-text)" }}>
            <span style={{ width: 20, height: 20, borderRadius: 10, background: "var(--accent)", color: "var(--accent-ink)", display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0, marginLeft: -3 }}>
              <Icon name="plus" size={14} stroke={2.8} />
            </span>
            <span className="grow" style={{ fontWeight: 600 }}>New plan</span>
          </button>
        )}
      </div>
      <div className="grow" />
      {nas && (
        <div className="side-card">
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="caps">{nas.name}</span>
            <span className="tiny faint">{nas.kind === "smb" ? "SMB" : "Folder"}</span>
          </div>
          <div className="meter">
            <div style={{ width: `${Math.round((1 - (nas.free ?? 0) / (nas.total ?? 1)) * 100)}%` }} />
          </div>
          <span className="small muted">
            {bytes((nas.total ?? 0) - (nas.free ?? 0))} used · {bytes(nas.free)} free
          </span>
        </div>
      )}
      <button className={`navlink${screen.name === "settings" ? " on" : ""}`} style={{ marginTop: 10 }} onClick={() => go({ name: "settings" })} title="Settings ⌘,">
        <Icon name="settings" />
        <span className="grow">Settings</span>
        <span className="key">⌘,</span>
      </button>
    </nav>
  );
}

function shortAgo(iso: string | null): string {
  if (!iso) return "";
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 3600) return `${Math.max(1, Math.round(s / 60))}m`;
  if (s < 86400) {
    const d = new Date(iso);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  }
  return ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][new Date(iso).getDay()];
}

function Toolbar({ back, forward, canBack, canForward }: { back: () => void; forward: () => void; canBack: boolean; canForward: boolean }) {
  const { ov, go, screen } = useApp();
  const [q, setQ] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        input.current?.focus();
        input.current?.select();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);
  const watching = ov?.plans.filter((p) => p.enabled && p.status !== "off").length ?? 0;
  const running = !!ov?.job;
  return (
    <header className="toolbar" data-tauri-drag-region>
      <button className="iconbtn" aria-label="Back" title="Back ⌘[" disabled={!canBack} onClick={back}>
        <Icon name="back" stroke={2} />
      </button>
      <button className="iconbtn" aria-label="Forward" title="Forward ⌘]" disabled={!canForward} onClick={forward}>
        <Icon name="forward" stroke={2} />
      </button>
      {screen.name !== "restore" && (
        <form
          className="search"
          style={{ marginLeft: 8 }}
          onSubmit={(e) => {
            e.preventDefault();
            if (q.trim()) go({ name: "restore", query: q.trim() });
          }}
        >
          <Icon name="search" size={15} stroke={2} />
          <input ref={input} aria-label="Find a file in any backup" placeholder="Find a file in any backup" value={q} onChange={(e) => setQ(e.target.value)} />
          <span className="kbd">⌘K</span>
        </form>
      )}
      <div className="spacer" data-tauri-drag-region />
      {watching > 0 && (
        <span className="row small muted" style={{ gap: 6 }}>
          <span className={running ? "dot spin" : "dot"} />
          {running ? `Backing up ${ov?.job?.planName}` : `Watching ${watching} plan${watching === 1 ? "" : "s"}`}
        </span>
      )}
      {running ? (
        <>
          <div className="progress" style={{ width: 90, height: 6 }} aria-label="Progress">
            <div className="solid" style={{ width: `${ov?.job ? pct(ov.job) : 0}%`, transition: "width 0.4s" }} />
          </div>
          <button className="btn" onClick={() => api.cancel(ov!.job!.id)} title="Stop this backup">
            <Icon name="stop" size={12} />
            Stop
          </button>
        </>
      ) : (
        (ov?.plans.length ?? 0) > 0 && (
          <button className="btn primary" onClick={() => api.backUpAll()} title="Back up every plan now ⌘B">
            <Icon name="up" size={14} stroke={2.2} />
            Back up now
          </button>
        )
      )}
    </header>
  );
}

export default function App() {
  const [ov, setOv] = useState<OverviewData | null>(null);
  const [cfg, setCfg] = useState<Config | null>(null);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [home, setHome] = useState("");
  const [past, setPast] = useState<Screen[]>([]);
  const [future, setFuture] = useState<Screen[]>([]);
  const [screen, setScreen] = useState<Screen>({ name: "overview" });

  const refresh = useCallback(async () => {
    const [o, c] = await Promise.all([api.overview(), api.config()]);
    setOv(o);
    setCfg(c);
    setJob(o.job);
    hideSplash();
  }, []);

  useEffect(() => {
    api.home().then(setHome);
    // Screenshot mode: open the screen the scene names. Read before the first load, so a scene
    // can keep the splash up.
    api.scene().then((sc) => {
      refresh();
      if (!sc) return;
      document.documentElement.dataset.scene = "1";
      const s = JSON.parse(sc) as { screen?: Screen; theme?: string; splash?: boolean };
      if (s.splash) document.documentElement.dataset.keepSplash = "1";
      if (s.theme) document.documentElement.dataset.theme = s.theme;
      if (s.screen) setScreen(s.screen);
    });
    const un1 = on("changed", () => refresh());
    const un2 = on<JobStatus>("job", (j) => setJob(j));
    const un3 = on<string>("navigate", (s) => go({ name: s } as Screen));
    // Times like "12 min ago" move on even when nothing happens.
    const t = window.setInterval(refresh, 30_000);
    return () => {
      un1();
      un2();
      un3();
      window.clearInterval(t);
    };
  }, [refresh]);

  const go = useCallback(
    (s: Screen) => {
      setScreen((cur) => {
        if (JSON.stringify(cur) === JSON.stringify(s)) return cur;
        setPast((p) => [...p.slice(-50), cur]);
        setFuture([]);
        return s;
      });
    },
    [],
  );
  const back = () => {
    const prev = past[past.length - 1];
    if (!prev) return;
    setPast(past.slice(0, -1));
    setFuture([screen, ...future]);
    setScreen(prev);
  };
  const forward = () => {
    const nxt = future[0];
    if (!nxt) return;
    setFuture(future.slice(1));
    setPast([...past, screen]);
    setScreen(nxt);
  };

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (!e.metaKey) return;
      const i = "1234".indexOf(e.key);
      if (i >= 0) {
        e.preventDefault();
        go({ name: NAV[i][0] } as Screen);
      } else if (e.key === ",") {
        e.preventDefault();
        go({ name: "settings" });
      } else if (e.key === "[") back();
      else if (e.key === "]") forward();
      else if (e.key.toLowerCase() === "n" && !e.shiftKey) {
        e.preventDefault();
        go({ name: "plans", isNew: true });
      } else if (e.key.toLowerCase() === "b") {
        e.preventDefault();
        api.backUpAll();
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });

  const ctx: AppCtx = { ov: ov && { ...ov, job }, cfg, job, home, screen, go, refresh };
  return (
    <Ctx.Provider value={ctx}>
      <ToastProvider>
        <div className="app">
          <Sidebar />
          <main className="main">
            <Toolbar back={back} forward={forward} canBack={past.length > 0} canForward={future.length > 0} />
            {!ov || !cfg ? null : screen.name === "overview" ? (
              <Overview />
            ) : screen.name === "plans" ? (
              <Plans key={`${screen.plan ?? ""}-${screen.isNew ?? ""}`} />
            ) : screen.name === "restore" ? (
              <Restore key={`${screen.plan ?? ""}-${screen.query ?? ""}`} />
            ) : screen.name === "activity" ? (
              <Activity />
            ) : screen.name === "destinations" ? (
              <Destinations />
            ) : (
              <Settings />
            )}
          </main>
        </div>
        <Tooltips />
      </ToastProvider>
    </Ctx.Provider>
  );
}
