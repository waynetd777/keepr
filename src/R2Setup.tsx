// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Cloudflare R2 setup from a setup token the person makes in Cloudflare's dashboard. Keepr uses
// it once to make (or, for a source, find) the bucket and a token for that bucket alone, then
// deletes the setup token.

import { useState } from "react";
import { api, type AwsMade } from "./api";
import { KeySetup } from "./KeySetup";

export function R2Setup({ mode, onMade }: { mode: "destination" | "source"; onMade: (m: AwsMade) => void }) {
  const [token, setToken] = useState("");
  const [account, setAccount] = useState("");
  return (
    <KeySetup
      mode={mode}
      intro={
        <>
          In Cloudflare, make a custom API token with two permissions: <b>Account › Workers R2 Storage › Edit</b> and{" "}
          <b>User › API Tokens › Edit</b>. Paste it here. Keepr uses it once to{" "}
          {mode === "source"
            ? "make a token that may only read the bucket you choose"
            : "make a private bucket and a token that works only in it"}
          , then deletes it.
        </>
      }
      link={{ label: "Open API Tokens in Cloudflare", url: "https://dash.cloudflare.com/profile/api-tokens" }}
      fields={
        <>
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
        </>
      }
      haveKey={!!token.trim()}
      none="There are no R2 buckets in this account."
      making={(b) => (mode === "source" ? `Making a read-only token for ${b}…` : "Making the bucket and its token…")}
      list={() => api.r2Buckets(token, account)}
      setup={async (b) => {
        const m = await api.r2SetupRun(token, account, b, mode);
        setToken("");
        return m;
      }}
      onMade={onMade}
    />
  );
}
