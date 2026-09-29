// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Backblaze B2 setup from the account's master application key, which Keepr uses once and
// doesn't keep. For a destination: a new private bucket that keeps only the latest version of
// each file. For a source: a bucket that's already there. Either way, a key for that bucket alone.

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api, type AwsMade } from "./api";
import { Icon } from "./icons";

export function B2Setup({ mode, onMade }: { mode: "destination" | "source"; onMade: (m: AwsMade) => void }) {
  const [keyId, setKeyId] = useState("");
  const [key, setKey] = useState("");
  const [bucket, setBucket] = useState("");
  const [buckets, setBuckets] = useState<string[] | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    if (mode === "destination") api.awsSetupInfo().then((i) => setBucket(i.bucket));
  }, [mode]);
  const run = async (doing: string, f: () => Promise<void>) => {
    setBusy(doing);
    setError("");
    try {
      await f();
    } catch (e) {
      setError(String(e));
    }
    setBusy("");
  };
  const made = (b: string) =>
    run(mode === "source" ? `Making a read-only key for ${b}…` : "Making the bucket and its key…", async () => {
      const m = await api.b2SetupRun(keyId, key, b, mode);
      setKey("");
      onMade(m);
    });
  const haveKey = !!(keyId.trim() && key.trim());
  return (
    <div className="card col" style={{ padding: 14, gap: 10, background: "var(--sunk)" }}>
      <span style={{ fontWeight: 600 }}>
        {mode === "source" ? "Let Keepr set up read-only access" : "New to this? Let Keepr set it up"}
      </span>
      <span className="small muted" style={{ lineHeight: 1.5 }}>
        {mode === "source"
          ? "Enter your account's master application key. Keepr uses it once to make a key that may only list and read the bucket you choose, and doesn't keep it."
          : "Enter your account's master application key. Keepr uses it once to make a private bucket that keeps only the latest version of each file (so tidying up frees space), and a key that works only in that bucket. It doesn't keep the master key."}{" "}
        In Backblaze it's under Application Keys › Master Application Key. Making a new one stops the old one working.
      </span>
      <div className="row" style={{ gap: 8 }}>
        <button className="btn small" onClick={() => openUrl("https://secure.backblaze.com/app_keys.htm")}>
          Open Application Keys in Backblaze
        </button>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "10px 12px" }}>
        <label className="field">
          <span>Master key ID</span>
          <input className="input mono" value={keyId} onChange={(e) => setKeyId(e.target.value)} />
        </label>
        <label className="field">
          <span>Master application key</span>
          <input className="input" type="password" value={key} onChange={(e) => setKey(e.target.value)} />
        </label>
        {mode === "destination" && (
          <label className="field" style={{ gridColumn: "span 2" }}>
            <span>New bucket's name</span>
            <input className="input mono" value={bucket} onChange={(e) => setBucket(e.target.value)} />
          </label>
        )}
      </div>
      <div className="row" style={{ gap: 8 }}>
        {mode === "destination" ? (
          <button className="btn primary" disabled={!haveKey || !bucket.trim() || !!busy} onClick={() => made(bucket)}>
            Set up
          </button>
        ) : (
          <button
            className="btn primary"
            disabled={!haveKey || !!busy}
            onClick={() => run("Asking for the buckets…", async () => setBuckets(await api.b2Buckets(keyId, key)))}
          >
            List my buckets
          </button>
        )}
        <span className="small muted grow">{busy}</span>
      </div>
      {buckets && (
        <div className="col" style={{ gap: 6 }}>
          <span className="small muted">{buckets.length ? "Choose the bucket to back up:" : "There are no buckets in this account."}</span>
          <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
            {buckets.map((b) => (
              <button key={b} className="btn small" disabled={!!busy} onClick={() => made(b)}>
                <Icon name="bucket" size={13} />
                {b}
              </button>
            ))}
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
