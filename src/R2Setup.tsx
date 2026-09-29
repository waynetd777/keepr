// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Cloudflare R2 setup from a setup token the person makes in Cloudflare's dashboard. Keepr uses
// it once to make (or, for a source, find) the bucket and a token for that bucket alone, then
// deletes the setup token.

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api, type AwsMade } from "./api";
import { Icon } from "./icons";

export function R2Setup({ mode, onMade }: { mode: "destination" | "source"; onMade: (m: AwsMade) => void }) {
  const [token, setToken] = useState("");
  const [account, setAccount] = useState("");
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
    run(mode === "source" ? `Making a read-only token for ${b}…` : "Making the bucket and its token…", async () => {
      const m = await api.r2SetupRun(token, account, b, mode);
      setToken("");
      onMade(m);
    });
  return (
    <div className="card col" style={{ padding: 14, gap: 10, background: "var(--sunk)" }}>
      <span style={{ fontWeight: 600 }}>
        {mode === "source" ? "Let Keepr set up read-only access" : "New to this? Let Keepr set it up"}
      </span>
      <span className="small muted" style={{ lineHeight: 1.5 }}>
        In Cloudflare, make a custom API token with two permissions: <b>Account › Workers R2 Storage › Edit</b> and{" "}
        <b>User › API Tokens › Edit</b>. Paste it here. Keepr uses it once to{" "}
        {mode === "source"
          ? "make a token that may only read the bucket you choose"
          : "make a private bucket and a token that works only in it"}
        , then deletes it.
      </span>
      <div className="row" style={{ gap: 8 }}>
        <button className="btn small" onClick={() => openUrl("https://dash.cloudflare.com/profile/api-tokens")}>
          Open API Tokens in Cloudflare
        </button>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "10px 12px" }}>
        <label className="field">
          <span>Setup token</span>
          <input className="input" type="password" value={token} onChange={(e) => setToken(e.target.value)} />
        </label>
        <label className="field">
          <span>Account ID</span>
          <input
            className="input mono"
            placeholder="Only if you have several"
            value={account}
            onChange={(e) => setAccount(e.target.value)}
          />
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
          <button className="btn primary" disabled={!token.trim() || !bucket.trim() || !!busy} onClick={() => made(bucket)}>
            Set up
          </button>
        ) : (
          <button
            className="btn primary"
            disabled={!token.trim() || !!busy}
            onClick={() => run("Asking for the buckets…", async () => setBuckets(await api.r2Buckets(token, account)))}
          >
            List my buckets
          </button>
        )}
        <span className="small muted grow">{busy}</span>
      </div>
      {buckets && (
        <div className="col" style={{ gap: 6 }}>
          <span className="small muted">
            {buckets.length ? "Choose the bucket to back up:" : "There are no R2 buckets in this account."}
          </span>
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
