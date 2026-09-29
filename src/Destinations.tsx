// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Destinations: where backups are kept, and the sheet for adding one (a folder or drive, or an
// SMB share, a cloud service's folder on this Mac, or an S3 bucket).

import { useEffect, useState } from "react";
import { api, type AwsMade, type Destination, type Place, type Tested } from "./api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { B2Setup } from "./B2Setup";
import { R2Setup } from "./R2Setup";
import { useApp } from "./context";
import { Icon } from "./icons";
import { DestIcon, SAVED_PASSWORD, Seg, Sheet, useAct, useSavedLogin } from "./ui";
import { bytes, tilde } from "./format";

export type Service = "aws" | "b2" | "r2" | "other";

/** The S3 service an endpoint belongs to, and what the form needs to rebuild it. */
export function serviceOf(endpoint: string): { service: Service; r2Account: string } {
  const host = endpoint.replace(/^[a-z]+:\/\//, "").split(/[/:]/)[0];
  if (!endpoint || host.endsWith(".amazonaws.com")) return { service: "aws", r2Account: "" };
  if (host.endsWith(".backblazeb2.com")) return { service: "b2", r2Account: "" };
  if (host.endsWith(".r2.cloudflarestorage.com")) return { service: "r2", r2Account: host.split(".")[0] };
  return { service: "other", r2Account: "" };
}

type Cloud = Awaited<ReturnType<typeof api.cloudFolders>>[number];

/** Which cloud folder a path is in, if any. */
function cloudOf(path: string, clouds: Cloud[]): Cloud | undefined {
  return clouds.find((c) => path === c.root || path.startsWith(c.root + "/"));
}

/** Makes a private bucket and a user that may only use it, by signing in to AWS in the browser
 *  (with the AWS CLI) or with a script pasted into AWS CloudShell. */
function AwsSetup({ region, setRegion, onMade }: { region: string; setRegion: (r: string) => void; onMade: (m: AwsMade) => void }) {
  const act = useAct();
  const [info, setInfo] = useState<{ cli: boolean; bucket: string } | null>(null);
  const [bucket, setBucket] = useState("");
  const [busy, setBusy] = useState("");
  const [shell, setShell] = useState(false);
  const [pasted, setPasted] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    api.awsSetupInfo().then((i) => {
      setInfo(i);
      setBucket(i.bucket);
    });
  }, []);
  const run = async (f: () => Promise<AwsMade>, doing: string) => {
    setBusy(doing);
    setError("");
    try {
      onMade(await f());
    } catch (e) {
      setError(String(e));
    }
    setBusy("");
  };
  const cloudShell = () =>
    act(async () => {
      const script = await api.awsSetupScript(region, bucket);
      await navigator.clipboard.writeText(script);
      setShell(true);
      await openUrl(`https://${region}.console.aws.amazon.com/cloudshell/home?region=${region}`);
    });
  if (!info) return null;
  return (
    <div className="card col" style={{ padding: 14, gap: 10, background: "var(--sunk)" }}>
      <span style={{ fontWeight: 600 }}>New to this? Let Keepr set it up</span>
      <span className="small muted" style={{ lineHeight: 1.5 }}>
        Keepr makes a private bucket and a user that can only reach that bucket, and keeps its key in your Keychain. You sign in with your
        own AWS login; Keepr doesn't keep it.
      </span>
      <div className="row" style={{ gap: 8 }}>
        <label className="field grow">
          <span>New bucket's name</span>
          <input className="input mono" value={bucket} onChange={(e) => setBucket(e.target.value)} />
        </label>
        <label className="field" style={{ width: 180 }}>
          <span>Region</span>
          <input className="input mono" placeholder="eu-west-1" value={region} onChange={(e) => setRegion(e.target.value)} />
        </label>
      </div>
      <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
        {info.cli && (
          <button
            className="btn primary"
            disabled={!!busy || !bucket || !region}
            onClick={() => run(() => api.awsSetupRun(region, bucket), "Sign in in your browser, then come back here…")}
          >
            Sign in to AWS and set up
          </button>
        )}
        <button className={`btn${info.cli ? "" : " primary"}`} disabled={!!busy || !bucket || !region} onClick={cloudShell}>
          {info.cli ? "Use CloudShell instead" : "Set up in AWS CloudShell"}
        </button>
        <span className="small muted grow">{busy}</span>
      </div>
      {shell && (
        <div className="col" style={{ gap: 6 }}>
          <span className="small muted" style={{ lineHeight: 1.5 }}>
            The setup is copied. In CloudShell, paste it (⌘V) and press Return. When it's done, copy the line starting{" "}
            <span className="mono">keepr-setup</span> and paste it here.
          </span>
          <div className="row" style={{ gap: 8 }}>
            <input className="input mono grow" placeholder="keepr-setup {…}" value={pasted} onChange={(e) => setPasted(e.target.value)} />
            <button
              className="btn"
              disabled={!!busy || !pasted.includes("keepr-setup")}
              onClick={() => run(() => api.awsSetupPaste(pasted), "Checking the new key…")}
            >
              Use it
            </button>
          </div>
        </div>
      )}
      {error && (
        <span className="small" style={{ color: "var(--red)" }}>
          {error}
        </span>
      )}
    </div>
  );
}

export function AddDestination({ onClose, editing }: { onClose: () => void; editing?: Destination }) {
  const { refresh, home } = useApp();
  const act = useAct();
  const [kind, setKind] = useState<"folder" | "smb" | "cloud" | "s3">(editing?.place.kind ?? "folder");
  const [clouds, setClouds] = useState<Cloud[]>([]);
  const [cloud, setCloud] = useState<Cloud | null>(null);
  const [inCloud, setInCloud] = useState("Keepr");
  const [servers, setServers] = useState<string[]>([]);
  const smb = editing?.place.kind === "smb" ? editing.place : null;
  const [server, setServer] = useState(smb?.server ?? "");
  const [share, setShare] = useState(smb?.share ?? "");
  const [shares, setShares] = useState<string[]>([]);
  const [user, setUser] = useState(smb?.user ?? "");
  const [password, setPassword] = useState("");
  const [folder, setFolder] = useState(smb?.folder ?? "/");
  const [path, setPath] = useState(editing?.place.kind === "folder" ? editing.place.path : "");
  const [keychain, setKeychain] = useState(true);
  const s3 = editing?.place.kind === "s3" ? editing.place : null;
  const [service, setService] = useState<Service>(serviceOf(s3?.endpoint ?? "").service);
  const [r2Account, setR2Account] = useState(serviceOf(s3?.endpoint ?? "").r2Account);
  const [endpoint, setEndpoint] = useState(s3?.endpoint ?? "");
  const [region, setRegion] = useState(s3?.region ?? "eu-west-1");
  const [bucket, setBucket] = useState(s3?.bucket ?? "");
  const [prefix, setPrefix] = useState(s3?.prefix ?? "");
  const [accessKey, setAccessKey] = useState(s3?.accessKey ?? "");
  const [secret, setSecret] = useState("");
  const [secretSaved, setSecretSaved] = useState(false);
  // The name: suggested from the place until someone types one.
  const [name, setName] = useState(editing?.name ?? "");
  const [named, setNamed] = useState(!!editing);
  const [tested, setTested] = useState<Tested | null>(null);
  const [busy, setBusy] = useState("");
  const savedFrom = useSavedLogin(
    server,
    user,
    setUser,
    setPassword,
    (s) => {
      setShares(s);
      if (!share && s.length) setShare(s.includes("Backups") ? "Backups" : s[0]);
    },
    setBusy,
  );
  // The saved password is used where the field still shows it.
  const pw = kind === "s3" ? secret || undefined : password === SAVED_PASSWORD ? undefined : password;

  useEffect(() => {
    api.cloudFolders().then((c) => {
      setClouds(c);
      // Changing a destination that's in a cloud folder opens on that folder.
      const p = editing?.place.kind === "folder" ? editing.place.path : "";
      const at = p && cloudOf(p, c);
      if (at) {
        setKind("cloud");
        setCloud(at);
        setInCloud(p.slice(at.root.length).replace(/^\/+/, ""));
      }
    });
    api.discoverServers().then((s) => {
      setServers(s);
      if (!server && s[0]) setServer(s[0]);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(
    () => setTested(null),
    [
      kind,
      server,
      share,
      user,
      password,
      folder,
      path,
      cloud,
      inCloud,
      service,
      r2Account,
      endpoint,
      region,
      bucket,
      prefix,
      accessKey,
      secret,
    ],
  );

  const cloudPath = cloud ? `${cloud.root}/${inCloud.trim().replace(/^\/+|\/+$/g, "")}`.replace(/\/$/, "") : "";
  const s3Endpoint =
    service === "aws"
      ? `https://s3.${region.trim()}.amazonaws.com`
      : service === "b2"
        ? `https://s3.${region.trim()}.backblazeb2.com`
        : service === "r2"
          ? `https://${r2Account.trim()}.r2.cloudflarestorage.com`
          : endpoint.trim();
  const place: Place =
    kind === "s3"
      ? {
          kind: "s3",
          endpoint: s3Endpoint,
          region: service === "r2" ? "auto" : region.trim(),
          bucket: bucket.trim(),
          prefix: prefix.trim().replace(/^\/+|\/+$/g, ""),
          accessKey: accessKey.trim(),
        }
      : kind === "smb"
        ? { kind: "smb", server: server.trim(), share: share.trim(), folder: folder.trim() || "/", user: user.trim() }
        : { kind: "folder", path: kind === "cloud" ? cloudPath : path };
  const ready =
    kind === "s3"
      ? !!(
          bucket.trim() &&
          accessKey.trim() &&
          (secret || s3 || secretSaved) &&
          (service === "r2" ? r2Account.trim() : service === "other" ? endpoint.trim() : region.trim())
        )
      : kind === "smb"
        ? !!(server && share)
        : kind === "cloud"
          ? !!cloud
          : !!path;
  useEffect(() => {
    if (named || !ready) return;
    const t = window.setTimeout(() => api.suggestName(place, editing?.id).then(setName), 200);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [named, ready, kind, server, share, folder, path, cloud, inCloud, service, r2Account, endpoint, bucket]);

  const test = async () => {
    setBusy("Connecting…");
    const t = await api.testPlace(place, pw).catch((e) => ({ ok: false, message: String(e), free: null, total: null, mbps: null }));
    setTested(t);
    setBusy("");
    return t;
  };
  const listShares = async () => {
    setBusy("Asking for its shares…");
    try {
      const s = await api.listShares(server, user, pw);
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
    const saved = await act(() =>
      api.saveDestination(
        { id: editing?.id ?? "", name: name.trim(), place, disconnectAfter: true },
        kind === "s3" || keychain ? pw : undefined,
      ),
    );
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
        <div
          style={{
            width: 240,
            flexShrink: 0,
            background: "var(--sunk)",
            borderRight: "1px solid var(--line)",
            padding: "14px 10px",
            display: "flex",
            flexDirection: "column",
            gap: 2,
          }}
        >
          {(
            [
              ["folder", "Folder or drive", "This Mac, USB, Thunderbolt", "drive"],
              ["smb", "SMB share", "A NAS or another computer", "server"],
              ["s3", "S3 bucket", "Amazon, Backblaze, R2, MinIO…", "bucket"],
            ] as const
          ).map(([k, title, sub, icon]) => (
            <button
              key={k}
              aria-pressed={kind === k}
              disabled={!!editing && editing.place.kind !== k}
              onClick={() => setKind(k)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                height: 44,
                padding: "0 10px",
                border: 0,
                borderRadius: 8,
                textAlign: "left",
                background: kind === k ? "var(--surface)" : "transparent",
                boxShadow: kind === k ? "var(--seg)" : "none",
                color: kind === k ? "var(--accent-text)" : "inherit",
              }}
            >
              <Icon name={icon} size={18} />
              <span className="col" style={{ gap: 0 }}>
                <span style={{ fontWeight: 600 }}>{title}</span>
                <span className="tiny muted">{sub}</span>
              </span>
            </button>
          ))}
          <div className="caps" style={{ margin: "16px 10px 6px" }}>
            Cloud folders on this Mac
          </div>
          {clouds.map((c) => {
            const on = kind === "cloud" && cloud?.root === c.root;
            return (
              <button
                key={c.root}
                disabled={!c.live || (!!editing && editing.place.kind !== "folder")}
                title={c.live ? c.root.replace(/^\/Users\/[^/]+/, "~") : c.why}
                aria-pressed={on}
                onClick={() => {
                  setKind("cloud");
                  setCloud(c);
                }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  height: 36,
                  padding: "0 10px",
                  border: 0,
                  borderRadius: 8,
                  textAlign: "left",
                  background: on ? "var(--surface)" : "transparent",
                  boxShadow: on ? "var(--seg)" : "none",
                  color: !c.live ? "var(--ink3)" : on ? "var(--accent-text)" : "inherit",
                }}
              >
                <Icon name="cloud" size={16} style={{ flexShrink: 0 }} />
                <span className="col" style={{ gap: 0, minWidth: 0 }}>
                  <span className="ellipsis" style={{ fontWeight: on ? 600 : 400 }}>
                    {c.name}
                  </span>
                  {!c.live && <span className="tiny faint">Off</span>}
                </span>
              </button>
            );
          })}
        </div>

        <div className="grow" style={{ padding: "20px 26px", display: "flex", flexDirection: "column", gap: 14 }}>
          {kind === "s3" ? (
            <>
              <span className="muted" style={{ lineHeight: 1.5 }}>
                A bucket on Amazon S3 or a service that works like it. Make the bucket and an access key in the service's console first; the
                key needs to read, write, list and delete in that bucket.
              </span>
              <Seg
                label="Service"
                value={service}
                onChange={setService}
                options={[
                  ["aws", "Amazon S3"],
                  ["b2", "Backblaze B2"],
                  ["r2", "Cloudflare R2"],
                  ["other", "Other"],
                ]}
              />
              {service === "aws" && !s3 && !secretSaved && (
                <AwsSetup
                  region={region}
                  setRegion={setRegion}
                  onMade={(m) => {
                    setRegion(m.region);
                    setBucket(m.bucket);
                    setAccessKey(m.accessKey);
                    setSecret("");
                    setSecretSaved(true);
                  }}
                />
              )}
              {service === "b2" && !s3 && !secretSaved && (
                <B2Setup
                  mode="destination"
                  onMade={(m) => {
                    setRegion(m.region);
                    setBucket(m.bucket);
                    setAccessKey(m.accessKey);
                    setSecret("");
                    setSecretSaved(true);
                  }}
                />
              )}
              {service === "r2" && !s3 && !secretSaved && (
                <R2Setup
                  mode="destination"
                  onMade={(m) => {
                    setR2Account(serviceOf(m.endpoint).r2Account);
                    setBucket(m.bucket);
                    setAccessKey(m.accessKey);
                    setSecret("");
                    setSecretSaved(true);
                  }}
                />
              )}
              {secretSaved && (
                <div role="status" className="banner good">
                  <Icon name="check" size={18} stroke={2.4} />
                  <span className="text">
                    Made {bucket}, and {service === "aws" ? "a user" : service === "r2" ? "a token" : "a key"} that can only use it. Its key
                    is in your Keychain. Add the destination to finish.
                  </span>
                </div>
              )}
              {service !== "other" && !s3 && !secretSaved && <span className="small muted">Or use a bucket and key you already have:</span>}
              <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "12px 14px" }}>
                {service === "r2" ? (
                  <label className="field" style={{ gridColumn: "span 2" }}>
                    <span>Account ID</span>
                    <input
                      className="input mono"
                      placeholder="From the R2 page in Cloudflare's dashboard"
                      value={r2Account}
                      onChange={(e) => setR2Account(e.target.value)}
                    />
                  </label>
                ) : service === "other" ? (
                  <>
                    <label className="field">
                      <span>Endpoint</span>
                      <input
                        className="input mono"
                        placeholder="https://s3.eu-central-1.wasabisys.com"
                        value={endpoint}
                        onChange={(e) => setEndpoint(e.target.value)}
                      />
                    </label>
                    <label className="field">
                      <span>Region</span>
                      <input className="input mono" placeholder="us-east-1" value={region} onChange={(e) => setRegion(e.target.value)} />
                    </label>
                  </>
                ) : (
                  <label className="field" style={{ gridColumn: "span 2" }}>
                    <span>Region</span>
                    <input
                      className="input mono"
                      placeholder={service === "b2" ? "eu-central-003" : "eu-west-1"}
                      value={region}
                      onChange={(e) => setRegion(e.target.value)}
                    />
                    {service === "b2" && (
                      <span className="tiny faint">The part after "s3." in the bucket's S3 endpoint, shown on its page in Backblaze</span>
                    )}
                  </label>
                )}
                <label className="field">
                  <span>Bucket</span>
                  <input className="input mono" value={bucket} onChange={(e) => setBucket(e.target.value)} />
                </label>
                <label className="field">
                  <span>Folder in the bucket</span>
                  <input className="input mono" placeholder="Its top" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
                </label>
                <label className="field">
                  <span>Access key ID</span>
                  <input className="input mono" value={accessKey} onChange={(e) => setAccessKey(e.target.value)} />
                </label>
                <label className="field">
                  <span>Secret access key</span>
                  <input
                    className="input"
                    type="password"
                    placeholder={s3 ? "Unchanged" : secretSaved ? "Saved in your Keychain" : ""}
                    value={secret}
                    onChange={(e) => setSecret(e.target.value)}
                  />
                </label>
              </div>
              <span className="small faint">
                The secret key is kept in your Keychain. {service === "aws" ? "Test says so if the bucket is in another region." : ""}
              </span>
            </>
          ) : kind === "cloud" && cloud ? (
            <>
              <span className="muted" style={{ lineHeight: 1.5 }}>
                Keepr writes the backup into {cloud.name}'s folder on this Mac, and{" "}
                {cloud.provider === "icloud" ? "iCloud" : cloud.name.split(" ")[0]} uploads it. The Mac keeps a copy until the service
                offloads it (turn on {cloud.provider === "icloud" ? "Optimise Mac Storage" : "Files On-Demand"} for that).
              </span>
              <label className="field">
                <span>Folder inside {cloud.name}</span>
                <input className="input mono" value={inCloud} onChange={(e) => setInCloud(e.target.value)} />
              </label>
              <span className="small faint mono ellipsis">{tilde(cloudPath, home)}</span>
              {cloud.free != null && <span className="small muted">{bytes(cloud.free)} free on this Mac</span>}
            </>
          ) : kind === "folder" ? (
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
                  <span className="caps">Servers</span>
                  <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                    {servers.map((s) => {
                      const on = server === s;
                      const short = s.replace(/\.local$/, "");
                      return (
                        <button
                          key={s}
                          aria-pressed={on}
                          onClick={() => setServer(s)}
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
                    value={server}
                    onChange={(e) => setServer(e.target.value)}
                  />
                </label>
                <label className="field">
                  <span>Share</span>
                  <div className="row" style={{ gap: 6 }}>
                    <input className="input grow" value={share} onChange={(e) => setShare(e.target.value)} />
                    <button className="btn" disabled={!server || !!busy} onClick={listShares} title="Ask the server which shares it has">
                      List
                    </button>
                  </div>
                </label>
                {shares.length > 0 && (
                  <div className="row" style={{ gridColumn: "span 2", flexWrap: "wrap", gap: 6 }}>
                    <span className="small muted">Shares on {server}:</span>
                    {shares.map((s) => (
                      <button key={s} className={`btn small${share === s ? " primary" : ""}`} onClick={() => setShare(s)}>
                        {s}
                      </button>
                    ))}
                  </div>
                )}
                <label className="field">
                  <span>User name</span>
                  <input className="input" value={user} onChange={(e) => setUser(e.target.value)} />
                </label>
                <label className="field">
                  <span>Password</span>
                  <input
                    className="input"
                    type="password"
                    placeholder={editing ? "Unchanged" : ""}
                    value={password}
                    onFocus={() => password === SAVED_PASSWORD && setPassword("")}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                  {savedFrom && password === SAVED_PASSWORD && (
                    <span className="tiny faint">Saved {savedFrom === "finder" ? "by Finder" : "by Keepr"} in your Keychain</span>
                  )}
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
          <label className="field">
            <span>Name</span>
            <input
              className="input"
              placeholder="Say which it is, e.g. OneDrive Personal"
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setNamed(e.target.value.trim() !== "");
              }}
            />
          </label>
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
  // The toolbar's Add destination button.
  useEffect(() => {
    const open = () => setAdding(true);
    window.addEventListener("keepr:add-destination", open);
    return () => window.removeEventListener("keepr:add-destination", open);
  }, []);
  return (
    <div className="content col" style={{ gap: 16 }}>
      <h1>Destinations</h1>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 16 }}>
        {ov?.destinations.map((d) => {
          const conf = cfg?.destinations.find((x) => x.id === d.id);
          return (
            <div key={d.id} className="card" style={{ padding: 18, display: "flex", flexDirection: "column", gap: 8, minHeight: 150 }}>
              <div className="row">
                <DestIcon kind={d.kind} label={d.kindLabel} />
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
        <button
          className="card"
          onClick={() => setAdding(true)}
          style={{
            minHeight: 150,
            border: "1.5px dashed var(--line2)",
            background: "transparent",
            color: "var(--accent-text)",
            fontWeight: 600,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
          }}
        >
          <Icon name="plus" size={14} stroke={2.4} />
          Add a destination
        </button>
      </div>
      {adding && <AddDestination onClose={() => setAdding(false)} />}
      {editing && <AddDestination editing={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
