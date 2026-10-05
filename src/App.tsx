// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The window: sidebar, toolbar, one history of places for back and forward, and the screens.

import { useCallback, useEffect, useRef, useState } from "react";
import { Ctx, useApp, type AppCtx, type Screen } from "./context";
import { api, on, type Config, type JobStatus, type Overview as OverviewData } from "./api";
import { Icon, Mark } from "./icons";
import { ClearButton, pct, StopButton, ToastProvider, Tooltips } from "./ui";
import Overview from "./Overview";
import Plans from "./Plans";
import Restore from "./Restore";
import Activity from "./Activity";
import Destinations from "./Destinations";
import Settings from "./Settings";
import Search from "./Search";
import PlansMap from "./PlansMap";
import { hideSplash } from "./splash";
import { HelpButton, HelpDrawer, openHelp, type HelpView } from "./help/HelpDrawer";

// Backup Plans is the overview of every plan; a plan's settings open from its card.
const NAV: [Screen["name"], string, string, string][] = [
  ["overview", "Backup Plans", "plans", "⌘1"],
  ["restore", "Restore", "restore", "⌘2"],
  ["activity", "Activity", "activity", "⌘3"],
  ["destinations", "Destinations", "server", "⌘4"],
];

function Sidebar() {
  const { screen, go } = useApp();
  return (
    <nav className="sidebar" aria-label="Sidebar">
      <div className="brand">
        <Mark size={28} />
        Keepr
      </div>
      <div className="nav">
        {NAV.map(([name, label, icon, key]) => (
          <button
            key={name}
            className={screen.name === name || (name === "overview" && (screen.name === "plans" || screen.name === "sizemap")) ? "on" : ""}
            onClick={() => go({ name } as Screen)}
            title={`${label} ${key}`}
          >
            <Icon name={icon} />
            <span className="grow">{label}</span>
            <span className="key">{key}</span>
          </button>
        ))}
      </div>
      <div className="grow" />
      <button
        className={`navlink${screen.name === "settings" ? " on" : ""}`}
        style={{ marginTop: 10 }}
        onClick={() => go({ name: "settings" })}
        title="Settings ⌘,"
      >
        <Icon name="settings" />
        <span className="grow">Settings</span>
        <span className="key">⌘,</span>
      </button>
    </nav>
  );
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
      <form
        className="search"
        style={{ marginLeft: 8 }}
        onSubmit={(e) => {
          e.preventDefault();
          if (q.trim()) go({ name: "search", query: q.trim() });
        }}
      >
        <Icon name="search" size={15} stroke={2} />
        <input
          ref={input}
          aria-label="Find a file in any backup"
          placeholder="Find a file in any backup"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && setQ("")}
        />
        {q ? (
          <ClearButton
            onClick={() => {
              setQ("");
              input.current?.focus();
            }}
          />
        ) : (
          <span className="kbd">⌘K</span>
        )}
      </form>
      <div className="spacer" data-tauri-drag-region />
      {watching > 0 && (
        <span className="row small muted" style={{ gap: 6 }}>
          <span className={running ? "dot spin" : "dot"} />
          {running
            ? ov?.job?.stopping
              ? `Stopping ${ov?.job?.planName}…`
              : `Backing up ${ov?.job?.planName}`
            : `Watching ${watching} plan${watching === 1 ? "" : "s"}`}
        </span>
      )}
      {running && (
        <>
          <div className="progress" style={{ width: 90, height: 6 }} aria-label="Progress">
            <div className="solid" style={{ width: `${ov?.job ? pct(ov.job) : 0}%`, transition: "width 0.4s" }} />
          </div>
          <StopButton job={ov!.job!} />
        </>
      )}
      {screen.name === "destinations" && (
        <button
          className="btn"
          onClick={() => window.dispatchEvent(new Event("keepr:add-destination"))}
          title="Add somewhere to keep backups"
        >
          <Icon name="plus" size={13} stroke={2.4} />
          Add destination
        </button>
      )}
      {(screen.name === "overview" || screen.name === "plans") && (ov?.plans.length ?? 0) > 0 && (
        <button className="btn" onClick={() => go({ name: "plans", isNew: true })} title="Make a new backup plan ⌘N">
          <Icon name="plus" size={13} stroke={2.4} />
          New plan
        </button>
      )}
      {!running && (ov?.plans.length ?? 0) > 0 && (
        <button className="btn primary" onClick={() => api.backUpAll()} title="Back up every plan now ⌘B">
          <Icon name="up" size={14} stroke={2.2} />
          Back up now
        </button>
      )}
      <HelpButton />
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

  const go = useCallback((s: Screen) => {
    setScreen((cur) => {
      if (JSON.stringify(cur) === JSON.stringify(s)) return cur;
      setPast((p) => [...p.slice(-50), cur]);
      setFuture([]);
      return s;
    });
  }, []);

  useEffect(() => {
    api.home().then(setHome);
    // Screenshot mode: open the screen the scene names. Read before the first load, so a scene
    // can keep the splash up.
    api.scene().then((sc) => {
      refresh();
      if (!sc) return;
      document.documentElement.dataset.scene = "1";
      const s = JSON.parse(sc) as { screen?: Screen; theme?: string; splash?: boolean; help?: HelpView };
      if (s.splash) document.documentElement.dataset.keepSplash = "1";
      if (s.theme) document.documentElement.dataset.theme = s.theme;
      if (s.screen) setScreen(s.screen);
      if (s.help) openHelp(s.help);
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
  }, [refresh, go]);

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
            ) : screen.name === "sizemap" ? (
              <PlansMap />
            ) : screen.name === "plans" ? (
              <Plans key={`${screen.plan ?? ""}-${screen.isNew ?? ""}`} />
            ) : screen.name === "restore" ? (
              <Restore key={`${screen.plan ?? ""}-${screen.query ?? ""}-${screen.focus?.path ?? ""}`} />
            ) : screen.name === "activity" ? (
              <Activity />
            ) : screen.name === "search" ? (
              <Search key={screen.query} />
            ) : screen.name === "destinations" ? (
              <Destinations />
            ) : (
              <Settings />
            )}
          </main>
          <HelpDrawer />
        </div>
        <Tooltips />
      </ToastProvider>
    </Ctx.Provider>
  );
}
