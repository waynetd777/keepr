// Adding an S3 bucket as a plan's source: Keepr lists and reads it, never writes to it. For
// Amazon, signing in lists the buckets and makes a user that may only read the chosen one.

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api, type Place } from "./api";
import { serviceOf, type Service } from "./Destinations";
import { B2Setup } from "./B2Setup";
import { R2Setup } from "./R2Setup";
import { Icon } from "./icons";
import { Seg, Sheet } from "./ui";

export function S3SourceSheet({ onAdd, onClose }: { onAdd: (p: Place) => void; onClose: () => void }) {
  const [service, setService] = useState<Service>("aws");
  const [region, setRegion] = useState("eu-west-1");
  const [r2Account, setR2Account] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [bucket, setBucket] = useState("");
  const [prefix, setPrefix] = useState("");
  const [accessKey, setAccessKey] = useState("");
  const [secret, setSecret] = useState("");
  const [secretSaved, setSecretSaved] = useState(false);
  const [buckets, setBuckets] = useState<string[] | null>(null);
  const [cli, setCli] = useState(false);
  const [shell, setShell] = useState(false);
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  useEffect(() => {
    api.awsSetupInfo().then((i) => setCli(i.cli));
    return () => void api.awsSetupEnd();
  }, []);
  useEffect(() => setMsg(null), [service, region, r2Account, endpoint, bucket, prefix, accessKey, secret]);

  const s3Endpoint = service === "aws" ? `https://s3.${region.trim()}.amazonaws.com` : service === "b2" ? `https://s3.${region.trim()}.backblazeb2.com` : service === "r2" ? `https://${r2Account.trim()}.r2.cloudflarestorage.com` : endpoint.trim();
  const place: Place = { kind: "s3", endpoint: s3Endpoint, region: service === "r2" ? "auto" : region.trim(), bucket: bucket.trim(), prefix: prefix.trim().replace(/^\/+|\/+$/g, ""), accessKey: accessKey.trim() };
  const ready = !!(bucket.trim() && accessKey.trim() && (secret || secretSaved) && serviceOf(s3Endpoint).service === service);

  const step = async (doing: string, f: () => Promise<void>) => {
    setBusy(doing);
    setMsg(null);
    try {
      await f();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
    setBusy("");
  };
  const made = (m: { region: string; bucket: string; accessKey: string }) => {
    setRegion(m.region);
    setBucket(m.bucket);
    setAccessKey(m.accessKey);
    setSecret("");
    setSecretSaved(true);
    setBuckets(null);
    setShell(false);
  };
  const check = () =>
    step("Listing the bucket…", async () => {
      const size = await api.checkS3Source(place, secret || undefined);
      if (secret) setSecretSaved(true);
      setMsg({ ok: true, text: `Keepr can read it: ${size}.${service === "aws" ? " Amazon charges for data leaving AWS (about $0.09 a GB), so the first backup costs that for all of it; later ones only for what changed." : ""}` });
    });

  return (
    <Sheet
      title="Back up an S3 bucket"
      subtitle="Keepr lists the bucket and downloads what's new or changed. It never writes to it."
      onClose={onClose}
      width={640}
      foot={
        <>
          <span className="grow small muted">{busy}</span>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn" disabled={!ready || !!busy} onClick={check}>
            Check
          </button>
          <button
            className="btn primary"
            disabled={!ready || !!busy}
            onClick={() =>
              step("Listing the bucket…", async () => {
                await api.checkS3Source(place, secret || undefined);
                onAdd(place);
                onClose();
              })
            }
          >
            Add
          </button>
        </>
      }
    >
      <div className="sheet-body">
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
        {service === "aws" && !secretSaved && (
          <div className="card col" style={{ padding: 14, gap: 10, background: "var(--sunk)" }}>
            <span style={{ fontWeight: 600 }}>Let Keepr set up read-only access</span>
            <span className="small muted" style={{ lineHeight: 1.5 }}>
              Sign in with your own AWS login and choose the bucket. Keepr makes a user that may only list and read it, and keeps that user's key in your Keychain.
            </span>
            <div className="row" style={{ gap: 8, alignItems: "flex-end" }}>
              <label className="field" style={{ width: 170 }}>
                <span>Region to sign in to</span>
                <input className="input mono" value={region} onChange={(e) => setRegion(e.target.value)} />
              </label>
              {cli && (
                <button className="btn primary" disabled={!!busy || !region.trim()} onClick={() => step("Sign in in your browser, then come back here…", async () => setBuckets(await api.awsBuckets(region)))}>
                  Sign in to AWS
                </button>
              )}
              <button className={`btn${cli ? "" : " primary"}`} disabled={!!busy} onClick={() => setShell(true)}>
                {cli ? "Use CloudShell instead" : "Set up in AWS CloudShell"}
              </button>
            </div>
            {buckets && (
              <div className="col" style={{ gap: 6 }}>
                <span className="small muted">{buckets.length ? "Choose the bucket to back up:" : "There are no buckets in this account."}</span>
                <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
                  {buckets.map((b) => (
                    <button key={b} className="btn small" disabled={!!busy} onClick={() => step(`Giving Keepr read-only access to ${b}…`, async () => made(await api.awsSetupRun(region, b, "source")))}>
                      <Icon name="bucket" size={13} />
                      {b}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {shell && (
              <div className="col" style={{ gap: 6 }}>
                <div className="row" style={{ gap: 8 }}>
                  <input className="input mono grow" placeholder="The bucket's name" value={bucket} onChange={(e) => setBucket(e.target.value)} />
                  <button
                    className="btn"
                    disabled={!bucket.trim()}
                    onClick={() =>
                      step("", async () => {
                        await navigator.clipboard.writeText(await api.awsSetupScript(region, bucket, "source"));
                        await openUrl(`https://${region}.console.aws.amazon.com/cloudshell/home?region=${region}`);
                      })
                    }
                  >
                    Copy the setup and open CloudShell
                  </button>
                </div>
                <span className="small muted">
                  Paste it in CloudShell (⌘V) and press Return, then copy the line starting <span className="mono">keepr-setup</span> back here:
                </span>
                <div className="row" style={{ gap: 8 }}>
                  <input className="input mono grow" placeholder="keepr-setup {…}" value={pasted} onChange={(e) => setPasted(e.target.value)} />
                  <button className="btn" disabled={!!busy || !pasted.includes("keepr-setup")} onClick={() => step("Checking the new key…", async () => made(await api.awsSetupPaste(pasted)))}>
                    Use it
                  </button>
                </div>
              </div>
            )}
          </div>
        )}
        {service === "b2" && !secretSaved && <B2Setup mode="source" onMade={made} />}
        {service === "r2" && !secretSaved && (
          <R2Setup
            mode="source"
            onMade={(m) => {
              setR2Account(serviceOf(m.endpoint).r2Account);
              made(m);
            }}
          />
        )}
        {secretSaved && service !== "other" && (
          <div role="status" className="banner good">
            <Icon name="check" size={18} stroke={2.4} />
            <span className="text">Keepr may now list and read {bucket}, and nothing more. Its key is in your Keychain.</span>
          </div>
        )}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "12px 14px" }}>
          {service === "r2" ? (
            <label className="field" style={{ gridColumn: "span 2" }}>
              <span>Account ID</span>
              <input className="input mono" value={r2Account} onChange={(e) => setR2Account(e.target.value)} />
            </label>
          ) : service === "other" ? (
            <>
              <label className="field">
                <span>Endpoint</span>
                <input className="input mono" placeholder="https://s3.eu-central-1.wasabisys.com" value={endpoint} onChange={(e) => setEndpoint(e.target.value)} />
              </label>
              <label className="field">
                <span>Region</span>
                <input className="input mono" placeholder="us-east-1" value={region} onChange={(e) => setRegion(e.target.value)} />
              </label>
            </>
          ) : service === "b2" ? (
            <label className="field" style={{ gridColumn: "span 2" }}>
              <span>Region</span>
              <input className="input mono" placeholder="eu-central-003" value={region} onChange={(e) => setRegion(e.target.value)} />
            </label>
          ) : null}
          <label className="field">
            <span>Bucket</span>
            <input className="input mono" value={bucket} onChange={(e) => setBucket(e.target.value)} />
          </label>
          <label className="field">
            <span>Only this folder in it</span>
            <input className="input mono" placeholder="All of it" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
          </label>
          <label className="field">
            <span>Access key ID</span>
            <input className="input mono" value={accessKey} onChange={(e) => setAccessKey(e.target.value)} />
          </label>
          <label className="field">
            <span>Secret access key</span>
            <input className="input" type="password" placeholder={secretSaved ? "Saved in your Keychain" : ""} value={secret} onChange={(e) => setSecret(e.target.value)} />
          </label>
        </div>
        {msg && (
          <div role="status" className={`banner ${msg.ok ? "good" : "bad"}`} style={{ alignItems: "flex-start" }}>
            <Icon name={msg.ok ? "check" : "warning"} size={18} stroke={2.4} style={{ flexShrink: 0 }} />
            <span className="text">{msg.text}</span>
          </div>
        )}
      </div>
    </Sheet>
  );
}
