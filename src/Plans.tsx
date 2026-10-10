// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A backup plan: what to keep, what to leave out, which versions to keep, where, when, full and
// incremental, and encryption. Changes to an existing plan are saved as they are made; a new
// plan is saved with Create.

import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Every, type Often, type Place, type Plan, type Retention } from "./api";
import { useApp } from "./context";
import { Icon } from "./icons";
import { S3SourceSheet } from "./S3Source";
import { DestIcon, PlanProgress, Seg, Sheet, Switch, useAct, useMenu, useToast } from "./ui";
import { SmbFields, useSmbForm } from "./SmbFields";
import { ago, bytes, next, tilde } from "./format";

const DEFAULT_RETENTION: Retention = { allHours: 24, dailyDays: 30, weeklyWeeks: 52, monthlyMonths: 0, keepDeletedDays: 90 };

function blankPlan(dest: string, excludes: string[]): Plan {
  return {
    id: "",
    name: "",
    enabled: true,
    sources: [],
    destination: dest,
    folder: "",
    schedule: { every: "hourly", at: "02:00", weekday: 0 },
    retention: DEFAULT_RETENTION,
    excludes,
    gitignore: true,
    skipCloudOnly: true,
    skipMarked: true,
    maxFileSize: 0,
    encrypted: true,
    fullEvery: "weekly",
    checkEvery: "weekly",
    conditions: { catchUp: true, onBattery: true, minBattery: 20, noHotspot: true, limitMbps: 0 },
    before: "",
    beforeMustSucceed: false,
    after: "",
  };
}

/** In a cloud service's sync folder on this Mac. */
function isCloud(path: string): boolean {
  return /\/Library\/(CloudStorage|Mobile Documents)\//.test(path + "/");
}

function placeLabel(p: Place, home: string): { title: string; sub: string } {
  if (p.kind === "folder") {
    const name = p.path.split("/").filter(Boolean).pop() ?? p.path;
    const vol = p.path.startsWith("/Volumes/") ? p.path.split("/")[2] : null;
    return {
      title: p.name?.trim() || name,
      sub: `${tilde(p.path, home)} · ${vol ? `on ${vol}` : isCloud(p.path) ? "cloud folder" : "folder on this Mac"}`,
    };
  }
  if (p.kind === "s3")
    return {
      title: p.name?.trim() || (p.prefix ? p.prefix.split("/").pop()! : p.bucket),
      sub: `s3://${p.bucket}${p.prefix ? `/${p.prefix}` : ""} · S3 bucket`,
    };
  const folder = p.folder.replace(/^\/+|\/+$/g, "");
  return {
    title: p.name?.trim() || (folder ? folder.split("/").pop()! : p.share),
    sub: `smb://${p.server}/${p.share}${folder ? `/${folder}` : ""} · SMB share`,
  };
}

/** A default name for each source that tells same-named folders apart: "Notes (Work)", "Notes (Home)". */
function defaultNames(sources: Place[], home: string): string[] {
  const base = sources.map((s) => placeLabel({ ...s, name: null }, home).title);
  return sources.map((s, i) => {
    if (base.filter((b) => b.toLowerCase() === base[i].toLowerCase()).length < 2) return base[i];
    const parts = (s.kind === "folder" ? s.path : s.kind === "smb" ? `${s.server}/${s.share}/${s.folder}` : `${s.bucket}/${s.prefix}`)
      .split("/")
      .filter(Boolean);
    return `${base[i]} (${parts[parts.length - 2] ?? (s.kind === "smb" ? s.server : "")})`;
  });
}

// ---- versions to keep ----

function Ticks({ r }: { r: Retention }) {
  // Sixty ticks from now into the past, denser where more versions are kept.
  const ticks = useMemo(() => {
    const out: { h: number; on: boolean }[] = [];
    for (let i = 0; i < 60; i++) {
      let on: boolean;
      let h: number;
      if (i < 12) [on, h] = [r.allHours > 0, 24];
      else if (i < 30) [on, h] = [r.dailyDays > 0 && i % 2 === 0, 20];
      else if (i < 48) [on, h] = [r.weeklyWeeks > 0 && i % 4 === 0, 16];
      else [on, h] = [(r.monthlyMonths === 0 || i < 48 + r.monthlyMonths / 2) && i % 6 === 0, 12];
      out.push({ h: on ? h : 4, on });
    }
    return out;
  }, [r]);
  return (
    <div className="ticks" aria-hidden="true">
      {ticks.map((t, i) => (
        <div key={i}>
          <div style={{ height: t.h, background: t.on ? "var(--accent)" : "var(--line2)" }} />
        </div>
      ))}
    </div>
  );
}

function KeepBox({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: number;
  options: [number, string][];
  onChange: (v: number) => void;
}) {
  return (
    <label className="keep-box">
      <span className="tiny faint">{label}</span>
      <select value={value} onChange={(e) => onChange(Number(e.target.value))}>
        {options.map(([v, t]) => (
          <option key={v} value={v}>
            {t}
          </option>
        ))}
      </select>
    </label>
  );
}

// ---- sheets ----

export function RecoverySheet({ plan, name, onClose }: { plan: string; name: string; onClose: () => void }) {
  const [key, setKey] = useState<string | null>(null);
  const toast = useToast();
  const act = useAct();
  useEffect(() => {
    // A key that can't be read says why, as a toast; the sheet then reads as before the first backup.
    act(() => api.recoveryKey(plan)).then((k) => setKey(k ?? null));
  }, [plan, act]);
  return (
    <Sheet
      title={`${name}: recovery key`}
      subtitle="If the password is ever lost, this key still opens the backup. Keep it somewhere other than this Mac: printed, or in a password manager."
      onClose={onClose}
      foot={
        <>
          <button
            className="btn"
            disabled={!key}
            onClick={() => key && navigator.clipboard.writeText(key).then(() => toast("Recovery key copied."))}
          >
            Copy
          </button>
          <span className="grow" />
          <button className="btn" onClick={onClose}>
            Later
          </button>
          <button
            className="btn primary"
            onClick={async () => {
              // Left open if it can't be noted, so the banner asking for it isn't silently wrong.
              if ((await act(() => api.recoverySaved(plan).then(() => true))) === undefined) return;
              onClose();
            }}
          >
            I've saved it
          </button>
        </>
      }
    >
      <div className="sheet-body">
        {key ? (
          <div
            className="mono"
            style={{
              fontSize: 18,
              letterSpacing: "0.04em",
              padding: "18px 16px",
              background: "var(--sunk)",
              borderRadius: 10,
              textAlign: "center",
              userSelect: "text",
              wordBreak: "break-all",
            }}
          >
            {key}
          </div>
        ) : (
          <span className="muted">The recovery key is made with the first backup. Back up once, then come back here.</span>
        )}
      </div>
    </Sheet>
  );
}

function PasswordSheet({ plan, onClose }: { plan: Plan; onClose: () => void }) {
  const [cur, setCur] = useState("");
  const [pw, setPw] = useState("");
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const toast = useToast();
  const ok = cur && pw.length >= 8 && pw === again;
  return (
    <Sheet
      title="Change password"
      subtitle="The recovery key keeps working. Nothing in the backup is rewritten."
      onClose={onClose}
      width={480}
      foot={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={!ok || busy}
            onClick={async () => {
              setBusy(true);
              setErr("");
              try {
                await api.changePassword(plan.id, cur, pw);
                toast("Password changed.");
                onClose();
              } catch (e) {
                setErr(String(e));
              }
              setBusy(false);
            }}
          >
            {busy ? "Changing…" : "Change password"}
          </button>
        </>
      }
    >
      <div className="sheet-body">
        <label className="field">
          <span>Current password, or the recovery key</span>
          <input className="input" type="password" value={cur} onChange={(e) => setCur(e.target.value)} autoFocus />
        </label>
        <label className="field">
          <span>New password (at least 8 characters)</span>
          <input className="input" type="password" value={pw} onChange={(e) => setPw(e.target.value)} />
        </label>
        <label className="field">
          <span>New password again</span>
          <input className="input" type="password" value={again} onChange={(e) => setAgain(e.target.value)} />
        </label>
        {err && <span style={{ color: "var(--red)" }}>{err}</span>}
      </div>
    </Sheet>
  );
}

export function SmbSourceSheet({ onAdd, onClose }: { onAdd: (p: Place) => void; onClose: () => void }) {
  const act = useAct();
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const f = useSmbForm({ onMessage: setMsg });
  const place: Place = { kind: "smb", ...f.smb };
  return (
    <Sheet
      title="Back up a folder on a share"
      subtitle="Keepr connects to the share when it backs up, and disconnects afterwards."
      onClose={onClose}
      width={560}
      foot={
        <>
          <span className="grow small muted">{msg}</span>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={!f.server || !f.share || busy}
            onClick={async () => {
              setBusy(true);
              setMsg("Connecting…");
              const t = await api.testPlace(place, f.pw).catch((e) => ({ ok: false, message: String(e) }));
              setBusy(false);
              if (!t.ok) return setMsg(t.message);
              setMsg("");
              // Without the password saved, the backup couldn't connect later: not added, and the toast says why.
              if (f.pw && (await act(() => api.saveSmbPassword(place.server, place.user, f.pw!).then(() => true))) === undefined) return;
              onAdd(place);
              onClose();
            }}
          >
            Add
          </button>
        </>
      }
    >
      <div className="sheet-body">
        <SmbFields f={f} busy={busy} />
      </div>
    </Sheet>
  );
}

/** Removing a source that has been backed up: keep its versions, or delete them (confirmed twice). */
function RemoveSourceSheet({
  name,
  where,
  dest,
  onClose,
  onKeep,
  onDelete,
}: {
  name: string;
  where: string;
  dest: string;
  onClose: () => void;
  onKeep: () => void;
  onDelete: () => void;
}) {
  return (
    <Sheet
      title={`Remove ${name} from this plan?`}
      subtitle={`Keepr stops backing up ${where}. It already has versions of it at ${dest}.`}
      width={560}
      onClose={onClose}
      foot={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <span className="grow" />
          <button
            className="btn danger"
            onClick={async () => {
              const { ask } = await import("@tauri-apps/plugin-dialog");
              const sure = await ask(
                `Delete every backed-up version of ${where} from ${dest}? This can't be undone: those files can no longer be restored.`,
                { title: `Delete ${name}'s backed-up data`, kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
              );
              if (sure) onDelete();
            }}
          >
            <Icon name="trash" size={14} />
            Delete its backed-up data…
          </button>
          <button className="btn primary" onClick={onKeep}>
            Keep its versions
          </button>
        </>
      }
    >
      <div className="sheet-body">
        <div className="col" style={{ gap: 8, lineHeight: 1.5 }}>
          <span>
            <b>Keep its versions:</b> they stay restorable, and your version rules thin them out over time.
          </span>
          <span>
            <b>Delete its backed-up data:</b> every version of it is taken out of every snapshot, and the space it used is freed.
          </span>
        </div>
      </div>
    </Sheet>
  );
}

// ---- the editor ----

export default function Plans() {
  const { cfg, ov, screen, go, home, refresh } = useApp();
  const act = useAct();
  const toast = useToast();
  const wantedId = screen.name === "plans" ? screen.plan : undefined;
  const isNew = screen.name === "plans" && !!screen.isNew;
  const existing = cfg?.plans.find((p) => p.id === (wantedId ?? cfg?.plans[0]?.id));
  const [plan, setPlan] = useState<Plan | null>(null);
  const [newPassword, setNewPassword] = useState("");
  const [newPassword2, setNewPassword2] = useState("");
  const [ruleText, setRuleText] = useState("");
  const [sheet, setSheet] = useState<"" | "smb" | "s3" | "password" | "recovery">("");
  const [clouds, setClouds] = useState<Awaited<ReturnType<typeof api.cloudFolders>>>([]);
  useEffect(() => {
    api.cloudFolders().then((c) => setClouds(c.filter((x) => x.live)));
  }, []);
  // After a rename, the backup's folder can still carry the old name: offer to bring it in line.
  const [folderFor, setFolderFor] = useState("");
  const [renamingFolder, setRenamingFolder] = useState(false);
  useEffect(() => {
    if (!plan?.id) return;
    const t = window.setTimeout(() => api.suggestPlanFolder(plan.name.trim() || "Plan", plan.id).then(setFolderFor), 400);
    return () => window.clearTimeout(t);
  }, [plan?.name, plan?.id]);
  const [removing, setRemoving] = useState<number | null>(null);
  const addMenu = useMenu();
  const moreMenu = useMenu();
  const saveTimer = useRef<number>(undefined);

  useEffect(() => {
    if (isNew) {
      api.defaultExcludes().then((ex) => setPlan(blankPlan(cfg?.destinations[0]?.id ?? "", ex)));
    } else if (existing) {
      setPlan(structuredClone(existing));
    }
    // Loaded once per plan; later saves come back through the config without replacing edits.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isNew, existing?.id]);

  if (!cfg) return null;
  if (!isNew && !existing) {
    return (
      <div className="content">
        <div className="empty">
          <h1>No plans yet</h1>
          <span>A plan says what to back up, where to, and how often.</span>
          <button className="btn primary" disabled={cfg.destinations.length === 0} onClick={() => go({ name: "plans", isNew: true })}>
            Make a plan
          </button>
          {cfg.destinations.length === 0 && (
            <a href="#" onClick={(e) => (e.preventDefault(), go({ name: "destinations", add: true }))}>
              First, add a destination
            </a>
          )}
        </div>
      </div>
    );
  }
  if (!plan) return null;

  const summary = ov?.plans.find((p) => p.id === plan.id);
  const created = !!plan.id && (summary?.snapshots ?? 0) > 0;
  const dest = cfg.destinations.find((d) => d.id === plan.destination);

  const update = (f: (p: Plan) => Plan) => {
    const nextPlan = f(structuredClone(plan));
    setPlan(nextPlan);
    if (!nextPlan.id) return; // a new plan is saved with Create
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      act(async () => {
        await api.savePlan(nextPlan);
        await refresh();
      });
    }, 500);
  };

  const create = async () => {
    if (plan.encrypted && (newPassword.length < 8 || newPassword !== newPassword2)) {
      toast(newPassword.length < 8 ? "Use a password of at least 8 characters." : "The two passwords aren't the same.");
      return;
    }
    const saved = await act(() =>
      api.savePlan(
        { ...plan, name: plan.name.trim() || placeLabel(plan.sources[0] ?? { kind: "folder", path: "Plan" }, home).title },
        plan.encrypted ? newPassword : undefined,
      ),
    );
    if (saved) {
      await refresh();
      go({ name: "plans", plan: saved.id });
      api.backUp(saved.id);
      toast(`${saved.name} is set up. Its first backup has started.`);
    }
  };

  const addFolders = async (from?: string) => {
    const paths = await api.chooseFolders(from ? "Choose folders in the cloud folder to back up" : "Choose folders to back up", true, from);
    if (paths.length)
      update((p) => ({
        ...p,
        sources: [
          ...p.sources,
          ...paths
            .filter((x) => !p.sources.some((s) => s.kind === "folder" && s.path === x))
            .map((path) => ({ kind: "folder" as const, path })),
        ],
        name: p.name || (paths[0].split("/").pop() ?? ""),
      }));
  };

  const r = plan.retention;
  const setR = (k: keyof Retention, v: number) => update((p) => ({ ...p, retention: { ...p.retention, [k]: v } }));
  const everyOptions: [Every, string][] = [
    ["minutes15", "Every 15 min"],
    ["hourly", "Hourly"],
    ["daily", "Daily"],
    ["weekly", "Weekly"],
    ["manual", "Only when I ask"],
  ];

  return (
    <div className="content col" style={{ gap: 18 }}>
      <div className="row" style={{ gap: 12 }}>
        <div className="grow col" style={{ gap: 4 }}>
          <label className="editable-title" title="Rename this plan">
            <input
              aria-label="Plan name"
              placeholder="Name this plan"
              value={plan.name}
              onChange={(e) => update((p) => ({ ...p, name: e.target.value }))}
            />
            <Icon name="pencil" size={16} />
          </label>
          <span className="small muted">
            {!plan.id
              ? "A new plan. It's saved when you create it."
              : summary?.lastSuccess
                ? `Last backup ${ago(summary.lastSuccess)}. ${plan.enabled && summary.nextRun ? `Next ${next(summary.nextRun)}.` : ""} Changes are saved as you make them.`
                : "Changes are saved as you make them."}
          </span>
        </div>
        {plan.id ? (
          <>
            <label className="row small muted" style={{ gap: 8 }}>
              Plan on
              <Switch
                label="Plan on"
                on={plan.enabled}
                onChange={async (v) => {
                  if (!v) {
                    const { ask } = await import("@tauri-apps/plugin-dialog");
                    const sure = await ask(
                      `Turn off ${plan.name}? It won't back up until you turn it on again. Its backups so far stay as they are.`,
                      { title: "Turn off this plan", kind: "warning", okLabel: "Turn Off", cancelLabel: "Cancel" },
                    );
                    if (!sure) return;
                  }
                  update((p) => ({ ...p, enabled: v }));
                }}
              />
            </label>
            <button className="iconbtn" aria-label="More" title="More" onClick={moreMenu.open}>
              <Icon name="more" />
            </button>
            <moreMenu.Menu>
              <button onClick={() => act(() => api.backUp(plan.id, true))}>Back up everything now (full)</button>
              <button onClick={() => act(() => api.checkNow(plan.id, false))}>Check a sample of the backup now</button>
              <button onClick={() => act(() => api.checkNow(plan.id, true))}>Check all of the backup now</button>
              <hr />
              <button
                onClick={async () => {
                  const { ask } = await import("@tauri-apps/plugin-dialog");
                  if (
                    await ask(
                      `Start a new backup for ${plan.name}? Use this only if the old one is gone for good: Keepr makes a new, empty backup at ${dest?.name}.`,
                      { title: "Start a new backup", kind: "warning" },
                    )
                  )
                    act(() => api.startNewBackup(plan.id));
                }}
              >
                Start a new backup…
              </button>
              <button
                style={{ color: "var(--red)" }}
                onClick={async () => {
                  const { ask } = await import("@tauri-apps/plugin-dialog");
                  if (
                    await ask(`Delete the plan ${plan.name}? Its backup stays at ${dest?.name} and isn't deleted.`, {
                      title: "Delete plan",
                      kind: "warning",
                    })
                  ) {
                    await act(() => api.deletePlan(plan.id));
                    await refresh();
                    go({ name: "overview" });
                  }
                }}
              >
                Delete plan…
              </button>
            </moreMenu.Menu>
            {ov?.job?.plan === plan.id || ov?.queued.some((q) => q.plan === plan.id) ? null : (
              <button className="btn primary" onClick={() => act(() => api.backUp(plan.id))}>
                <Icon name="up" size={14} stroke={2.2} />
                Back up now
              </button>
            )}
          </>
        ) : (
          <>
            <button className="btn" onClick={() => go({ name: "overview" })}>
              Cancel
            </button>
            <button className="btn primary" disabled={plan.sources.length === 0 || !plan.destination} onClick={create}>
              Create and back up
            </button>
          </>
        )}
      </div>

      {plan.id && (ov?.job?.plan === plan.id || ov?.queued.some((q) => q.plan === plan.id)) && (
        <div className="card" style={{ padding: "14px 18px" }}>
          <PlanProgress job={ov?.job?.plan === plan.id ? ov.job : null} queued={ov?.queued.find((q) => q.plan === plan.id)} />
        </div>
      )}
      <div className="plan-grid">
        <div className="col" style={{ gap: 16 }}>
          <section className="card section">
            <div className="row" style={{ alignItems: "baseline" }}>
              <h2>What to keep</h2>
              <span className="muted grow">
                {plan.sources.length === 0 ? "Nothing yet" : `${plan.sources.length} source${plan.sources.length === 1 ? "" : "s"}`}
              </span>
            </div>
            {plan.sources.map((s, i) => {
              const l = placeLabel(s, home);
              const fallback = defaultNames(plan.sources, home)[i];
              return (
                <div key={i} className="source-row">
                  <Icon
                    name={s.kind === "smb" ? "server" : s.kind === "s3" ? "bucket" : isCloud(s.path) ? "cloud" : "folder"}
                    size={20}
                    style={{ color: "var(--accent)" }}
                  />
                  <div className="grow col" style={{ gap: 1 }}>
                    <input
                      aria-label="Source name"
                      title="Rename this source"
                      className="input source-name"
                      value={s.name ?? ""}
                      placeholder={fallback}
                      onChange={(e) =>
                        update((p) => ({ ...p, sources: p.sources.map((x, j) => (j === i ? { ...x, name: e.target.value || null } : x)) }))
                      }
                      style={{ border: 0, padding: 0, height: 20, fontWeight: 600, background: "transparent" }}
                    />
                    <span className="small muted ellipsis">{l.sub}</span>
                  </div>
                  <button
                    className="iconbtn"
                    aria-label={`Remove ${l.title}`}
                    title="Remove source"
                    onClick={() => (created ? setRemoving(i) : update((p) => ({ ...p, sources: p.sources.filter((_, j) => j !== i) })))}
                  >
                    <Icon name="close" size={14} stroke={2} />
                  </button>
                </div>
              );
            })}
            <div className="row" style={{ gap: 12 }}>
              <button className="btn dashed" onClick={addMenu.open}>
                <Icon name="plus" size={13} stroke={2.4} />
                Add a source
              </button>
              <span className="small faint">Keepr only reads your sources. It never changes or moves them.</span>
            </div>
            <addMenu.Menu width={290}>
              <button onClick={() => addFolders()}>
                <Icon name="folder" />
                <span className="grow">Folder or drive…</span>
              </button>
              <button onClick={() => setSheet("smb")}>
                <Icon name="server" />
                <span className="grow">SMB share on the network…</span>
              </button>
              {clouds.map((c) => (
                <button key={c.root} onClick={() => addFolders(c.root)}>
                  <Icon name="cloud" />
                  <span className="grow">In {c.name}…</span>
                </button>
              ))}
              <button onClick={() => setSheet("s3")}>
                <Icon name="bucket" />
                <span className="grow">S3 bucket…</span>
              </button>
            </addMenu.Menu>
          </section>

          <section className="card section">
            <h2>What to leave out</h2>
            <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
              {plan.excludes.map((x, i) => (
                <span key={i} className="rule">
                  {x}
                  <button
                    aria-label={`Remove rule ${x}`}
                    onClick={() => update((p) => ({ ...p, excludes: p.excludes.filter((_, j) => j !== i) }))}
                  >
                    <Icon name="close" size={10} stroke={2.6} />
                  </button>
                </span>
              ))}
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  const t = ruleText.trim();
                  if (t) update((p) => ({ ...p, excludes: [...p.excludes, t] }));
                  setRuleText("");
                }}
              >
                <input
                  className="input mono"
                  style={{ height: 26, width: 150, borderStyle: "dashed", borderRadius: 13 }}
                  placeholder="+ Rule, e.g. *.iso"
                  value={ruleText}
                  onChange={(e) => setRuleText(e.target.value)}
                  title="A name (*.tmp), a folder name ending in / (build/), or a path starting ~/ or /"
                />
              </form>
            </div>
            <label className="check">
              <input type="checkbox" checked={plan.gitignore} onChange={(e) => update((p) => ({ ...p, gitignore: e.target.checked }))} />
              Follow .gitignore files in projects
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={plan.skipCloudOnly}
                onChange={(e) => update((p) => ({ ...p, skipCloudOnly: e.target.checked }))}
              />
              Skip files that are only in the cloud (they aren't downloaded)
            </label>
            {plan.skipCloudOnly && plan.sources.some((s) => s.kind === "folder" && isCloud(s.path)) && (
              <span className="small muted" style={{ marginTop: -4, paddingLeft: 24, lineHeight: 1.5 }}>
                This plan backs up a cloud folder: files that are only online there won't be backed up. Untick this to download and back
                them up too (they stay downloaded afterwards).
              </span>
            )}
            <label className="check">
              <input type="checkbox" checked={plan.skipMarked} onChange={(e) => update((p) => ({ ...p, skipMarked: e.target.checked }))} />
              Skip what apps mark as not needing a backup, as Time Machine does
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={plan.maxFileSize > 0}
                onChange={(e) => update((p) => ({ ...p, maxFileSize: e.target.checked ? 4 * 1024 ** 3 : 0 }))}
              />
              Skip files larger than
              <select
                className="input"
                style={{ height: 24, fontSize: 12 }}
                disabled={plan.maxFileSize === 0}
                value={plan.maxFileSize || 4 * 1024 ** 3}
                onChange={(e) => update((p) => ({ ...p, maxFileSize: Number(e.target.value) }))}
              >
                {[1, 2, 4, 10, 50].map((g) => (
                  <option key={g} value={g * 1024 ** 3}>
                    {g} GB
                  </option>
                ))}
              </select>
            </label>
          </section>

          <section className="card section">
            <div className="row" style={{ alignItems: "baseline" }}>
              <h2>Versions to keep</h2>
              {summary && summary.snapshots > 0 && <span className="muted">{summary.snapshots.toLocaleString()} snapshots now</span>}
            </div>
            <Ticks r={r} />
            <div className="row tiny faint" style={{ justifyContent: "space-between", marginTop: -6 }}>
              <span>Now</span>
              <span>1 day</span>
              <span>1 month</span>
              <span>1 year</span>
              {/* A year is the mark before this one, so only a longer history gets a label here. */}
              <span>{r.monthlyMonths === 0 ? "Forever" : r.monthlyMonths > 12 ? `${r.monthlyMonths / 12} years` : ""}</span>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 10 }}>
              <KeepBox
                label="Every backup"
                value={r.allHours}
                onChange={(v) => setR("allHours", v)}
                options={[
                  [0, "not kept"],
                  [24, "for 24 hours"],
                  [48, "for 2 days"],
                  [168, "for a week"],
                ]}
              />
              <KeepBox
                label="One a day"
                value={r.dailyDays}
                onChange={(v) => setR("dailyDays", v)}
                options={[
                  [0, "not kept"],
                  [7, "for 7 days"],
                  [14, "for 14 days"],
                  [21, "for 21 days"],
                  [30, "for 30 days"],
                  [90, "for 90 days"],
                ]}
              />
              <KeepBox
                label="One a week"
                value={r.weeklyWeeks}
                onChange={(v) => setR("weeklyWeeks", v)}
                options={[
                  [0, "not kept"],
                  [4, "for 4 weeks"],
                  [12, "for 3 months"],
                  [26, "for 6 months"],
                  [52, "for 12 months"],
                ]}
              />
              <KeepBox
                label="One a month"
                value={r.monthlyMonths}
                onChange={(v) => setR("monthlyMonths", v)}
                options={[
                  [12, "for a year"],
                  [24, "for 2 years"],
                  [60, "for 5 years"],
                  [0, "forever"],
                ]}
              />
            </div>
            <label className="check muted">
              <input type="checkbox" checked={r.keepDeletedDays > 0} onChange={(e) => setR("keepDeletedDays", e.target.checked ? 90 : 0)} />
              Keep deleted files for at least 90 days, whatever the rules above say
            </label>
          </section>
        </div>

        <div className="col" style={{ gap: 16 }}>
          <section className="card section" style={{ gap: 10 }}>
            <h2>Where</h2>
            {cfg.destinations.length === 0 ? (
              <button className="btn primary" onClick={() => go({ name: "destinations", add: true })}>
                Add a destination
              </button>
            ) : (
              <div className="row" style={{ gap: 12 }}>
                <div className="tile" style={{ width: 38, height: 38 }}>
                  <DestIcon
                    kind={ov?.destinations.find((d) => d.id === plan.destination)?.kind}
                    label={ov?.destinations.find((d) => d.id === plan.destination)?.kindLabel}
                  />
                </div>
                <div className="grow col" style={{ gap: 2 }}>
                  <select
                    className="input"
                    value={plan.destination}
                    disabled={created}
                    title={created ? "A backup stays where it was made. Make a new plan to keep one elsewhere." : undefined}
                    onChange={(e) => update((p) => ({ ...p, destination: e.target.value }))}
                    style={{ fontWeight: 600 }}
                  >
                    {cfg.destinations.map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.name}
                      </option>
                    ))}
                  </select>
                  <span className="small muted ellipsis">
                    {dest?.place.kind === "smb"
                      ? `smb://${dest.place.server}/${dest.place.share}${dest.place.folder && dest.place.folder !== "/" ? dest.place.folder : ""}`
                      : dest?.place.kind === "folder"
                        ? tilde(dest.place.path, home)
                        : dest?.place.kind === "s3"
                          ? `s3://${dest.place.bucket}${dest.place.prefix ? `/${dest.place.prefix}` : ""}`
                          : ""}
                    {plan.folder ? ` / ${plan.folder}` : ""}
                  </span>
                  {created && plan.name.trim() && folderFor && folderFor !== plan.folder && dest?.place.kind !== "s3" && (
                    <span className="small muted row" style={{ gap: 8, marginTop: 4, flexWrap: "wrap" }}>
                      Its folder there is still called “{plan.folder}”.
                      <button
                        className="btn small"
                        disabled={renamingFolder}
                        onClick={async () => {
                          setRenamingFolder(true);
                          const n = await act(() => api.renamePlanFolder(plan.id));
                          setRenamingFolder(false);
                          if (n) {
                            setPlan((p) => (p ? { ...p, folder: n } : p));
                            toast(`Renamed to ${n}.`);
                            refresh();
                          }
                        }}
                      >
                        {renamingFolder ? "Renaming…" : `Rename to “${folderFor}”`}
                      </button>
                    </span>
                  )}
                </div>
              </div>
            )}
            <a href="#" className="small" onClick={(e) => (e.preventDefault(), go({ name: "destinations", add: true }))}>
              Add another destination
            </a>
          </section>

          <section className="card section">
            <h2>When</h2>
            <Seg
              label="Schedule"
              value={plan.schedule.every}
              options={everyOptions}
              onChange={(v) => update((p) => ({ ...p, schedule: { ...p.schedule, every: v } }))}
            />
            {(plan.schedule.every === "daily" || plan.schedule.every === "weekly") && (
              <div className="row small">
                {plan.schedule.every === "weekly" && (
                  <select
                    className="input"
                    value={plan.schedule.weekday}
                    onChange={(e) => update((p) => ({ ...p, schedule: { ...p.schedule, weekday: Number(e.target.value) } }))}
                  >
                    {["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"].map((d, i) => (
                      <option key={d} value={i}>
                        {d}s
                      </option>
                    ))}
                  </select>
                )}
                at
                <input
                  className="input mono"
                  type="time"
                  value={plan.schedule.at}
                  onChange={(e) => update((p) => ({ ...p, schedule: { ...p.schedule, at: e.target.value } }))}
                />
              </div>
            )}
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "8px 16px" }}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={plan.conditions.catchUp}
                  onChange={(e) => update((p) => ({ ...p, conditions: { ...p.conditions, catchUp: e.target.checked } }))}
                />
                Catch up after sleep or when the destination comes back
              </label>
              <div style={{ display: "grid", gap: 8 }}>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={plan.conditions.onBattery}
                    onChange={(e) => update((p) => ({ ...p, conditions: { ...p.conditions, onBattery: e.target.checked } }))}
                  />
                  Back up while on battery
                </label>
                <label className="check" style={{ paddingLeft: 22, opacity: plan.conditions.onBattery ? 1 : 0.5 }}>
                  <input
                    type="checkbox"
                    disabled={!plan.conditions.onBattery}
                    checked={plan.conditions.minBattery > 0}
                    onChange={(e) => update((p) => ({ ...p, conditions: { ...p.conditions, minBattery: e.target.checked ? 20 : 0 } }))}
                  />
                  Wait when battery is below 20%
                </label>
              </div>
              <label className="check">
                <input
                  type="checkbox"
                  checked={plan.conditions.noHotspot}
                  onChange={(e) => update((p) => ({ ...p, conditions: { ...p.conditions, noHotspot: e.target.checked } }))}
                />
                Not on a personal hotspot
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={plan.conditions.limitMbps > 0}
                  onChange={(e) => update((p) => ({ ...p, conditions: { ...p.conditions, limitMbps: e.target.checked ? 20 : 0 } }))}
                />
                Limit speed to
                <select
                  className="input"
                  style={{ height: 24, fontSize: 12 }}
                  disabled={plan.conditions.limitMbps === 0}
                  value={plan.conditions.limitMbps || 20}
                  onChange={(e) => update((p) => ({ ...p, conditions: { ...p.conditions, limitMbps: Number(e.target.value) } }))}
                >
                  {[5, 10, 20, 50, 100].map((v) => (
                    <option key={v} value={v}>
                      {v} MB/s
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </section>

          <section className="card section" style={{ gap: 10 }}>
            <h2>Full and incremental</h2>
            <p className="muted" style={{ lineHeight: 1.5 }}>
              The first backup copies everything. After that, each backup stores only what changed, but every snapshot restores as a
              complete copy.
            </p>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 10 }}>
              <label className="field">
                <span className="tiny faint">Re-read every file, not just changed ones</span>
                <select
                  className="input"
                  value={plan.fullEvery}
                  onChange={(e) => update((p) => ({ ...p, fullEvery: e.target.value as Often }))}
                >
                  <option value="weekly">Every week</option>
                  <option value="monthly">Every month</option>
                  <option value="never">Never</option>
                </select>
              </label>
              <label className="field">
                <span className="tiny faint">Check stored data can be read back</span>
                <select
                  className="input"
                  value={plan.checkEvery}
                  onChange={(e) => update((p) => ({ ...p, checkEvery: e.target.value as Often }))}
                >
                  <option value="weekly">A sample each week</option>
                  <option value="monthly">All of it each month</option>
                  <option value="never">Never</option>
                </select>
              </label>
            </div>
          </section>

          <section className="card section" style={{ gap: 10 }}>
            <h2>Before each backup</h2>
            <p className="muted" style={{ lineHeight: 1.5 }}>
              A command to run first, for example one that downloads a device's own backups into a folder this plan backs up. Its output
              goes into the backup's log.
            </p>
            <input
              className="input mono"
              aria-label="Command to run before each backup"
              placeholder="e.g. python3 ~/scripts/fetch.py ~/Backups/device"
              value={plan.before ?? ""}
              onChange={(e) => update((p) => ({ ...p, before: e.target.value }))}
            />
            {(plan.before ?? "").trim() !== "" && (
              <label className="field">
                <span className="tiny faint">If it fails</span>
                <select
                  className="input"
                  value={plan.beforeMustSucceed ? "stop" : "carry"}
                  onChange={(e) => update((p) => ({ ...p, beforeMustSucceed: e.target.value === "stop" }))}
                >
                  <option value="carry">Back up anyway, and mark the backup with a warning</option>
                  <option value="stop">Don't back up</option>
                </select>
              </label>
            )}
          </section>

          <section className="card section" style={{ gap: 10 }}>
            <h2>After each backup</h2>
            <p className="muted" style={{ lineHeight: 1.5 }}>
              A command to run when each backup ends, however it went, for example one that tells a home server. It gets the result in{" "}
              <span className="mono">$KEEPR_RESULT</span> (ok, warning, failed, waiting or cancelled), with{" "}
              <span className="mono">$KEEPR_PLAN</span>, <span className="mono">$KEEPR_MESSAGE</span> and{" "}
              <span className="mono">$KEEPR_LAST_SUCCESS</span>. Its output goes into the backup's log.
            </p>
            <input
              className="input mono"
              aria-label="Command to run after each backup"
              placeholder="e.g. ~/scripts/report.sh"
              value={plan.after ?? ""}
              onChange={(e) => update((p) => ({ ...p, after: e.target.value }))}
            />
          </section>

          <section className="card section" style={{ gap: 10 }}>
            <div className="row">
              <h2 className="grow">Encryption</h2>
              {created || plan.id ? (
                <span
                  className="row small"
                  style={{ gap: 5, fontWeight: 600, color: plan.encrypted ? "var(--accent-text)" : "var(--ink3)" }}
                >
                  {plan.encrypted && <Icon name="lock" size={13} stroke={2.2} />}
                  {plan.encrypted ? "On" : "Off"}
                </span>
              ) : (
                <Switch label="Encryption" on={plan.encrypted} onChange={(v) => update((p) => ({ ...p, encrypted: v }))} />
              )}
            </div>
            <p className="muted" style={{ lineHeight: 1.5 }}>
              {plan.encrypted
                ? `Files are encrypted on this Mac before they leave it. The password is in your Keychain; ${dest?.name ?? "the destination"} only ever sees scrambled data.`
                : "Files are stored as they are, compressed. Anyone who can open the destination can read them."}
            </p>
            {!plan.id && plan.encrypted && (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 10 }}>
                <label className="field">
                  <span>Password</span>
                  <input className="input" type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)} />
                </label>
                <label className="field">
                  <span>Again</span>
                  <input className="input" type="password" value={newPassword2} onChange={(e) => setNewPassword2(e.target.value)} />
                </label>
              </div>
            )}
            {plan.id && plan.encrypted && (
              <div className="row" style={{ gap: 8 }}>
                <button className="btn" disabled={!created} onClick={() => setSheet("password")}>
                  Change password…
                </button>
                <button className="btn" disabled={!created} onClick={() => setSheet("recovery")}>
                  <Icon name="key" size={14} stroke={1.9} />
                  Save recovery key…
                </button>
              </div>
            )}
            {plan.encrypted && (
              <div className="banner" style={{ alignItems: "flex-start", fontSize: 12, lineHeight: 1.45 }}>
                <Icon name="warning" size={15} stroke={2} style={{ flexShrink: 0, marginTop: 1 }} />
                <span className="text">Without the password or the recovery key, nobody can restore these files, including you.</span>
              </div>
            )}
          </section>
          {summary && summary.repoBytes > 0 && (
            <span className="small faint">
              This backup takes {bytes(summary.repoBytes)} at {summary.destination}.
            </span>
          )}
        </div>
      </div>

      {removing !== null && plan.sources[removing] && (
        <RemoveSourceSheet
          name={plan.sources[removing].name?.trim() || defaultNames(plan.sources, home)[removing]}
          where={placeLabel(plan.sources[removing], home).sub.split(" · ")[0]}
          dest={dest?.name ?? "the destination"}
          onClose={() => setRemoving(null)}
          onKeep={() => {
            const i = removing;
            setRemoving(null);
            update((p) => ({ ...p, sources: p.sources.filter((_, j) => j !== i) }));
          }}
          onDelete={async () => {
            const i = removing;
            const src = plan.sources[i];
            setRemoving(null);
            update((p) => ({ ...p, sources: p.sources.filter((_, j) => j !== i) }));
            if ((await act(() => api.removeSourceData(plan.id, src))) === undefined) return;
            toast("Removing its backed-up data. Activity shows how it's going.");
          }}
        />
      )}
      {sheet === "smb" && (
        <SmbSourceSheet
          onClose={() => setSheet("")}
          onAdd={(s) => update((p) => ({ ...p, sources: [...p.sources, s], name: p.name || (s.kind === "smb" ? s.share : "") }))}
        />
      )}
      {sheet === "s3" && (
        <S3SourceSheet
          onClose={() => setSheet("")}
          onAdd={(s) => update((p) => ({ ...p, sources: [...p.sources, s], name: p.name || (s.kind === "s3" ? s.bucket : "") }))}
        />
      )}
      {sheet === "password" && <PasswordSheet plan={plan} onClose={() => setSheet("")} />}
      {sheet === "recovery" && <RecoverySheet plan={plan.id} name={plan.name} onClose={() => setSheet("")} />}
    </div>
  );
}
