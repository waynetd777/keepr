// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The fields for an SMB share, the same wherever one is asked for: as a destination, and as a
// folder to back up. The servers found on the network, a login already saved for the one chosen,
// its shares, and a folder in the share.

import { useEffect, useState } from "react";
import { api, type Smb } from "./api";
import { Icon } from "./icons";
import { SAVED_PASSWORD, useSavedLogin } from "./ui";

export type SmbForm = ReturnType<typeof useSmbForm>;

/** The share's fields and what goes with them. `onMessage` shows what is happening ("" when it's
 *  done); `onError` says why the shares couldn't be listed (onMessage, unless given). */
export function useSmbForm({
  initial,
  pickServer = false,
  preferShare,
  onMessage,
  onError = onMessage,
}: {
  initial?: Smb | null;
  /** Choose the first server found when none is filled in yet. */
  pickServer?: boolean;
  /** The share chosen from a list when it's there, else the first. */
  preferShare?: string;
  onMessage: (m: string) => void;
  onError?: (m: string) => void;
}) {
  const [servers, setServers] = useState<string[]>([]);
  const [server, setServer] = useState(initial?.server ?? "");
  const [share, setShare] = useState(initial?.share ?? "");
  const [shares, setShares] = useState<string[]>([]);
  const [user, setUser] = useState(initial?.user ?? "");
  const [password, setPassword] = useState("");
  const [folder, setFolder] = useState(initial?.folder ?? "/");
  useEffect(() => {
    api.discoverServers().then(
      (s) => {
        setServers(s);
        if (pickServer && s[0]) setServer((cur) => cur || s[0]);
      },
      () => {},
    );
    // Once, when the form opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // A list of shares fills in the share, unless one is already there.
  const gotShares = (s: string[]) => {
    setShares(s);
    if (s.length) setShare((cur) => cur || (preferShare && s.includes(preferShare) ? preferShare : s[0]));
  };
  const savedFrom = useSavedLogin(server, user, setUser, setPassword, gotShares, onMessage);
  // The saved password is used where the field still shows it.
  const pw = password === SAVED_PASSWORD ? undefined : password;
  const smb: Smb = { server: server.trim(), share: share.trim(), folder: folder.trim() || "/", user: user.trim() };
  const listShares = async () => {
    onMessage("Asking for its shares…");
    try {
      gotShares(await api.listShares(server, user, pw));
      onMessage("");
    } catch (e) {
      onMessage("");
      onError(String(e));
    }
  };
  return {
    servers,
    server,
    setServer,
    share,
    setShare,
    shares,
    user,
    setUser,
    password,
    setPassword,
    folder,
    setFolder,
    savedFrom,
    pw,
    smb,
    listShares,
  };
}

/** The servers found, then server, share, login and folder. */
export function SmbFields({ f, busy, editing }: { f: SmbForm; busy?: boolean; editing?: boolean }) {
  return (
    <>
      {f.servers.length > 0 && (
        <div className="col" style={{ gap: 6 }}>
          <span className="caps">Servers</span>
          <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
            {f.servers.map((s) => {
              const on = f.server === s;
              const short = s.replace(/\.local$/, "");
              return (
                <button
                  key={s}
                  aria-pressed={on}
                  onClick={() => f.setServer(s)}
                  style={{
                    flex: 1,
                    minWidth: 160,
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "10px 12px",
                    border: on ? "1.5px solid var(--accent)" : "1px solid var(--line)",
                    borderRadius: 10,
                    background: on ? "var(--accent-soft)" : "var(--surface)",
                    textAlign: "left",
                  }}
                >
                  <Icon name="server" size={18} />
                  <span className="col" style={{ gap: 0 }}>
                    <span style={{ fontWeight: 600 }}>{short}</span>
                    <span className="tiny muted">{s}</span>
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
          <input
            className="input mono"
            placeholder="keep-nas.local or 192.168.1.20"
            value={f.server}
            onChange={(e) => f.setServer(e.target.value)}
          />
        </label>
        <label className="field">
          <span>Share</span>
          <div className="row" style={{ gap: 6 }}>
            <input className="input grow" value={f.share} onChange={(e) => f.setShare(e.target.value)} />
            <button className="btn" disabled={!f.server || busy} onClick={f.listShares} title="Ask the server which shares it has">
              List
            </button>
          </div>
        </label>
        {f.shares.length > 0 && (
          <div className="row" style={{ gridColumn: "span 2", flexWrap: "wrap", gap: 6 }}>
            <span className="small muted">Shares on {f.server}:</span>
            {f.shares.map((s) => (
              <button key={s} className={`btn small${f.share === s ? " primary" : ""}`} onClick={() => f.setShare(s)}>
                {s}
              </button>
            ))}
          </div>
        )}
        <label className="field">
          <span>User name</span>
          <input className="input" value={f.user} onChange={(e) => f.setUser(e.target.value)} />
        </label>
        <label className="field">
          <span>Password</span>
          <input
            className="input"
            type="password"
            placeholder={editing ? "Unchanged" : ""}
            value={f.password}
            onFocus={() => f.password === SAVED_PASSWORD && f.setPassword("")}
            onChange={(e) => f.setPassword(e.target.value)}
          />
          {f.savedFrom && f.password === SAVED_PASSWORD && (
            <span className="tiny faint">Saved {f.savedFrom === "finder" ? "by Finder" : "by Keepr"} in your Keychain</span>
          )}
        </label>
        <label className="field" style={{ gridColumn: "span 2" }}>
          <span>Folder in the share</span>
          <input className="input mono" placeholder="/ for all of it" value={f.folder} onChange={(e) => f.setFolder(e.target.value)} />
        </label>
      </div>
    </>
  );
}
