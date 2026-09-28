// Destinations: where backups are kept, and the sheet for adding one (a folder or drive, or an
// SMB share, with the cloud services shown as coming later).

import { useEffect, useState } from "react";
import { api, type Destination, type Place, type Tested } from "./api";
import { useApp } from "./App";
import { Icon } from "./icons";
import { Sheet, useAct } from "./ui";
import { bytes, tilde } from "./format";

const LATER: [string, string][] = [
  ["SFTP server", "terminal"],
  ["Amazon S3 or compatible", "bucket"],
  ["Google Drive", "cloud"],
  ["OneDrive", "cloud"],
  ["iCloud Drive", "cloud"],
];

export function AddDestination({ onClose, editing }: { onClose: () => void; editing?: Destination }) {
  const { refresh, home } = useApp();
  const act = useAct();
  const [kind, setKind] = useState<"folder" | "smb">(editing?.place.kind ?? "smb");
  const [servers, setServers] = useState<string[]>([]);
  const smb = editing?.place.kind === "smb" ? editing.place : null;
  const [server, setServer] = useState(smb?.server ?? "");
  const [share, setShare] = useState(smb?.share ?? "");
  const [shares, setShares] = useState<string[]>([]);
  const [user, setUser] = useState(smb?.user ?? "");
  const [password, setPassword] = useState("");
  const [folder, setFolder] = useState(smb?.folder ?? "/Keepr");
  const [path, setPath] = useState(editing?.place.kind === "folder" ? editing.place.path : "");
  const [keychain, setKeychain] = useState(true);
  const [tested, setTested] = useState<Tested | null>(null);
  const [busy, setBusy] = useState("");

  useEffect(() => {
    api.discoverServers().then((s) => {
      setServers(s);
      if (!server && s[0]) setServer(`${s[0]}.local`);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => setTested(null), [kind, server, share, user, password, folder, path]);

  const place: Place = kind === "smb" ? { kind: "smb", server: server.trim(), share: share.trim(), folder: folder.trim() || "/", user: user.trim() } : { kind: "folder", path };
  const ready = kind === "smb" ? !!(server && share) : !!path;

  const test = async () => {
    setBusy("Connecting…");
    const t = await api.testPlace(place, password).catch((e) => ({ ok: false, message: String(e), free: null, total: null, mbps: null }));
    setTested(t);
    setBusy("");
    return t;
  };
  const listShares = async () => {
    setBusy("Asking for its shares…");
    try {
      const s = await api.listShares(server, user, password);
      setShares(s);
      if (!share && s.length) setShare(s.includes("Backups") ? "Backups" : s[0]);
    } catch (e) {
      setTested({ ok: false, message: String(e), free: null, total: null, mbps: null });
    }
    setBusy("");
  };
  const add = async () => {
    const t = tested?.ok ? tested : await test();
    if (!t.ok) return;
    const name = kind === "smb" ? server.replace(/\.local$/, "") : path.split("/").filter(Boolean).pop() ?? "Folder";
    const saved = await act(() => api.saveDestination({ id: editing?.id ?? "", name: editing?.name ?? name, place, disconnectAfter: true }, keychain ? password : undefined));
    if (saved) {
      await refresh();
      onClose();
    }
  };

  return (
    <Sheet
      title={editing ? `Change ${editing.name}` : "Add a destination"}
      subtitle="Where Keepr stores your backups. One destination can hold many plans."
      width={860}
      onClose={onClose}
      foot={
        <>
          <span className="grow small muted">{busy}</span>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn" disabled={!ready || !!busy} onClick={test}>
            Test
          </button>
          <button className="btn primary" disabled={!ready || !!busy} onClick={add}>
            {editing ? "Save" : "Add destination"}
          </button>
        </>
      }
    >
      <div style={{ display: "flex", minHeight: 440 }}>
        <div style={{ width: 240, flexShrink: 0, background: "var(--sunk)", borderRight: "1px solid var(--line)", padding: "14px 10px", display: "flex", flexDirection: "column", gap: 2 }}>
          {(
            [
              ["folder", "Folder or drive", "This Mac, USB, Thunderbolt", "drive"],
              ["smb", "SMB share", "A NAS or another computer", "server"],
            ] as const
          ).map(([k, title, sub, icon]) => (
            <button
              key={k}
              aria-pressed={kind === k}
              disabled={!!editing && editing.place.kind !== k}
              onClick={() => setKind(k)}
              style={{ display: "flex", alignItems: "center", gap: 10, height: 44, padding: "0 10px", border: 0, borderRadius: 8, textAlign: "left", background: kind === k ? "var(--surface)" : "transparent", boxShadow: kind === k ? "var(--seg)" : "none", color: kind === k ? "var(--accent-text)" : "inherit" }}
            >
              <Icon name={icon} size={18} />
              <span className="col" style={{ gap: 0 }}>
                <span style={{ fontWeight: 600 }}>{title}</span>
                <span className="tiny muted">{sub}</span>
              </span>
            </button>
          ))}
          <div className="caps" style={{ margin: "16px 10px 6px" }}>
            Coming later
          </div>
          {LATER.map(([name, icon]) => (
            <button key={name} disabled style={{ display: "flex", alignItems: "center", gap: 10, height: 32, padding: "0 10px", border: 0, borderRadius: 8, background: "transparent", color: "var(--ink3)", textAlign: "left" }}>
              <Icon name={icon} />
              {name}
            </button>
          ))}
        </div>

        <div className="grow" style={{ padding: "20px 26px", display: "flex", flexDirection: "column", gap: 14 }}>
          {kind === "folder" ? (
            <>
              <span className="muted" style={{ lineHeight: 1.5 }}>
                A folder on this Mac or on a drive. For an external drive, Keepr waits while it's unplugged and catches up when it's back.
              </span>
              <div className="row">
                <div className="input grow mono ellipsis" style={{ display: "flex", alignItems: "center" }}>
                  {path ? tilde(path, home) : "No folder chosen"}
                </div>
                <button
                  className="btn"
                  onClick={async () => {
                    const [f] = await api.chooseFolders("Keep backups in this folder", false, path || undefined);
                    if (f) setPath(f);
                  }}
                >
                  Choose…
                </button>
              </div>
            </>
          ) : (
            <>
              {servers.length > 0 && (
                <div className="col" style={{ gap: 6 }}>
                  <span className="caps">Found on your network</span>
                  <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                    {servers.map((s) => {
                      const on = server === `${s}.local`;
                      return (
                        <button
                          key={s}
                          aria-pressed={on}
                          onClick={() => setServer(`${s}.local`)}
                          style={{ flex: 1, minWidth: 160, display: "flex", alignItems: "center", gap: 10, padding: "10px 12px", border: on ? "1.5px solid var(--accent)" : "1px solid var(--line)", borderRadius: 10, background: on ? "var(--accent-soft)" : "var(--surface)", textAlign: "left" }}
                        >
                          <Icon name="server" size={18} />
                          <span className="col" style={{ gap: 0 }}>
                            <span style={{ fontWeight: 600 }}>{s}</span>
                            <span className="tiny muted">{s}.local</span>
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
              <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "12px 14px" }}>
                <label className="field">
                  <span>Server</span>
                  <input className="input mono" placeholder="keep-nas.local or 192.168.1.20" value={server} onChange={(e) => setServer(e.target.value)} />
                </label>
                <label className="field">
                  <span>Share</span>
                  <div className="row" style={{ gap: 6 }}>
                    <input className="input grow" list="dest-shares" value={share} onChange={(e) => setShare(e.target.value)} />
                    <button className="btn" disabled={!server || !!busy} onClick={listShares} title="Ask the server which shares it has">
                      List
                    </button>
                  </div>
                  <datalist id="dest-shares">
                    {shares.map((s) => (
                      <option key={s} value={s} />
                    ))}
                  </datalist>
                </label>
                <label className="field">
                  <span>User name</span>
                  <input className="input" value={user} onChange={(e) => setUser(e.target.value)} />
                </label>
                <label className="field">
                  <span>Password</span>
                  <input className="input" type="password" placeholder={editing ? "Unchanged" : ""} value={password} onChange={(e) => setPassword(e.target.value)} />
                </label>
                <label className="field" style={{ gridColumn: "span 2" }}>
                  <span>Folder in the share</span>
                  <input className="input mono" value={folder} onChange={(e) => setFolder(e.target.value)} />
                </label>
              </div>
              <label className="check">
                <input type="checkbox" checked={keychain} onChange={(e) => setKeychain(e.target.checked)} />
                Save the password in my Keychain
              </label>
            </>
          )}
          {tested && (
            <div role="status" className={`banner ${tested.ok ? "good" : "bad"}`} style={{ alignItems: "flex-start" }}>
              <Icon name={tested.ok ? "check" : "warning"} size={18} stroke={2.4} style={{ flexShrink: 0 }} />
              <span className="col text" style={{ gap: 2 }}>
                <span style={{ fontWeight: 600 }}>{tested.message}</span>
                {tested.ok && (
                  <span className="small muted">
                    {tested.free != null && `${bytes(tested.free)} free`}
                    {tested.mbps != null && ` · ${tested.mbps.toFixed(0)} MB/s in a quick test`}
                  </span>
                )}
              </span>
            </div>
          )}
        </div>
      </div>
    </Sheet>
  );
}

export default function Destinations() {
  const { ov, cfg, screen, refresh } = useApp();
  const act = useAct();
  const [adding, setAdding] = useState(screen.name === "destinations" && !!screen.add);
  const [editing, setEditing] = useState<Destination | null>(null);
  return (
    <div className="content col" style={{ gap: 16 }}>
      <h1>Destinations</h1>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 16 }}>
        {ov?.destinations.map((d) => {
          const conf = cfg?.destinations.find((x) => x.id === d.id);
          return (
            <div key={d.id} className="card" style={{ padding: 18, display: "flex", flexDirection: "column", gap: 8, minHeight: 150 }}>
              <div className="row">
                <Icon name={d.kind === "smb" ? "server" : "drive"} size={20} />
                <span className="grow" style={{ fontWeight: 600, fontSize: 15 }}>
                  {d.name}
                </span>
                <span className="small" style={{ color: d.connection === "missing" ? "var(--amber)" : "var(--ink2)" }}>
                  {d.connection === "missing" ? "Not connected" : d.connection === "on demand" ? "Connects when needed" : "Connected"}
                </span>
              </div>
              <span className="small muted mono ellipsis">{d.place}</span>
              {d.total != null && (
                <>
                  <div className="meter">
                    <div style={{ width: `${Math.round((1 - (d.free ?? 0) / d.total) * 100)}%` }} />
                  </div>
                  <span className="small muted">
                    {bytes(d.keeprBytes)} from Keepr · {bytes(d.free)} free of {bytes(d.total)}
                  </span>
                </>
              )}
              <span className="small faint">{d.plans.length ? `Holds ${d.plans.join(", ")}` : "No plans use it yet"}</span>
              <span className="grow" />
              <div className="row" style={{ gap: 8 }}>
                <button className="btn small" onClick={() => conf && setEditing(conf)}>
                  Change…
                </button>
                <button
                  className="btn small danger"
                  disabled={d.plans.length > 0}
                  title={d.plans.length ? "Plans keep their backups here" : undefined}
                  onClick={async () => {
                    await act(() => api.deleteDestination(d.id));
                    refresh();
                  }}
                >
                  Remove
                </button>
              </div>
            </div>
          );
        })}
        <button className="card" onClick={() => setAdding(true)} style={{ minHeight: 150, border: "1.5px dashed var(--line2)", background: "transparent", color: "var(--accent-text)", fontWeight: 600, display: "flex", alignItems: "center", justifyContent: "center", gap: 6 }}>
          <Icon name="plus" size={14} stroke={2.4} />
          Add a destination
        </button>
      </div>
      {adding && <AddDestination onClose={() => setAdding(false)} />}
      {editing && <AddDestination editing={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
