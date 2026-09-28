// The menu-bar window: how things stand, the backup running now, each plan's state, and the
// few things worth doing from the menu bar.

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, on, type JobStatus, type Overview } from "./api";
import { Mark } from "./icons";
import { statusDot } from "./App";
import { secondsLeft, when } from "./format";

export default function Tray() {
  const [ov, setOv] = useState<Overview | null>(null);
  const [job, setJob] = useState<JobStatus | null>(null);
  useEffect(() => {
    const load = () => api.overview().then((o) => (setOv(o), setJob(o.job)));
    load();
    const u1 = on("changed", load);
    const u2 = on<JobStatus>("job", setJob);
    const u3 = on("tray-opened", load);
    return () => (u1(), u2(), u3());
  }, []);
  // Size the window to what it shows.
  useEffect(() => {
    const el = document.querySelector(".traywin") as HTMLElement | null;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      import("@tauri-apps/api/dpi").then(({ LogicalSize }) => getCurrentWindow().setSize(new LogicalSize(360, Math.ceil(el.scrollHeight) + 2)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ov]);
  if (!ov) return null;
  const others = ov.plans.filter((p) => p.id !== job?.plan);
  const attention = ov.plans.filter((p) => ["failed", "waiting", "stale"].includes(p.status));
  const headline = job ? "Keeping your files" : ov.plans.length === 0 ? "Nothing set up yet" : attention.length ? `${attention.length} plan${attention.length === 1 ? " needs" : "s need"} attention` : "Everything is kept";
  const sub = job ? `One backup running${others.every((p) => p.status === "ok") ? " · everything else is up to date" : ""}` : attention[0]?.message ?? (ov.plans.length ? "All plans are up to date" : "Open Keepr to set up a backup");
  const pct = job && job.bytesToRead ? Math.floor((100 * job.bytesRead) / job.bytesToRead) : 0;
  const hide = () => getCurrentWindow().hide();
  const Item = ({ label, keys, onClick, strong }: { label: string; keys?: string; onClick: () => void; strong?: boolean }) => (
    <button className="menu-item" style={strong ? { background: "var(--accent-soft)", color: "var(--accent-text)", fontWeight: 600 } : undefined} onClick={() => (onClick(), hide())}>
      <span className="grow">{label}</span>
      {keys && <span className="tiny faint">{keys}</span>}
    </button>
  );
  return (
    <div className="traywin">
      <div style={{ padding: "16px 18px 14px", display: "flex", flexDirection: "column", gap: 3, borderBottom: "1px solid var(--line)" }}>
        <div className="row" style={{ gap: 10 }}>
          <Mark size={22} />
          <span style={{ fontFamily: "var(--display)", fontWeight: 650, letterSpacing: "-0.02em", fontSize: 18 }}>{headline}</span>
        </div>
        <span className="muted ellipsis">{sub}</span>
      </div>
      {job && (
        <div style={{ padding: "14px 18px", display: "flex", flexDirection: "column", gap: 8, borderBottom: "1px solid var(--line)" }}>
          <div className="row" style={{ alignItems: "baseline" }}>
            <span className="grow" style={{ fontWeight: 600 }}>
              {job.planName}
            </span>
            <span className="mono muted" style={{ fontSize: 11 }}>
              {pct}%{job.etaSecs != null ? ` · ${secondsLeft(job.etaSecs).replace("about ", "")}` : ""}
            </span>
          </div>
          <div className="progress" style={{ height: 6 }}>
            <div className="solid" style={{ width: `${pct}%` }} />
          </div>
          <span className="small faint">
            {job.stage} · {job.filesToRead.toLocaleString()} files to read
          </span>
        </div>
      )}
      {others.length > 0 && (
        <div style={{ padding: 8, display: "flex", flexDirection: "column", gap: 1, borderBottom: "1px solid var(--line)" }}>
          {others.map((p) => (
            <button key={p.id} className="menu-item" style={{ height: 34 }} onClick={() => (api.showMain("overview"), hide())}>
              <span className={statusDot(p.status)} />
              <span className="grow ellipsis">{p.name}</span>
              <span className="small" style={{ color: ["failed", "waiting", "stale"].includes(p.status) ? "var(--amber)" : "var(--ink2)" }}>
                {["failed", "waiting", "stale"].includes(p.status) ? p.message.slice(0, 34) : when(p.lastSuccess)}
              </span>
            </button>
          ))}
        </div>
      )}
      <div style={{ padding: 6, display: "flex", flexDirection: "column", gap: 1 }}>
        {ov.plans.length > 0 && <Item label="Back up all now" keys="⌘B" strong onClick={() => api.backUpAll()} />}
        {ov.plans.length > 0 && <Item label="Pause backups for an hour" onClick={() => api.pauseHour()} />}
        {ov.plans.length > 0 && <Item label="Restore a file…" keys="⌘R" onClick={() => api.showMain("restore")} />}
        <div style={{ height: 1, background: "var(--line)", margin: "4px 8px" }} />
        <Item label="Open Keepr" keys="⌘O" onClick={() => api.showMain()} />
        <Item label="Settings…" keys="⌘," onClick={() => api.showMain("settings")} />
        <Item label="Quit Keepr" keys="⌘Q" onClick={() => api.quit()} />
      </div>
      <div style={{ padding: "10px 18px", background: "var(--sunk)", borderTop: "1px solid var(--line)", fontSize: 11, color: "var(--ink3)" }}>Backups keep running when the window is closed.</div>
    </div>
  );
}
