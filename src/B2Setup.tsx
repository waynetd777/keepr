// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Backblaze B2 setup from the account's master application key, which Keepr uses once and
// doesn't keep. For a destination: a new private bucket that keeps only the latest version of
// each file. For a source: a bucket that's already there. Either way, a key for that bucket alone.

import { useState } from "react";
import { api, type AwsMade } from "./api";
import { KeySetup } from "./KeySetup";

export function B2Setup({ mode, onMade }: { mode: "destination" | "source"; onMade: (m: AwsMade) => void }) {
  const [keyId, setKeyId] = useState("");
  const [key, setKey] = useState("");
  return (
    <KeySetup
      mode={mode}
      intro={
        <>
          {mode === "source"
            ? "Enter your account's master application key. Keepr uses it once to make a key that may only list and read the bucket you choose, and doesn't keep it."
            : "Enter your account's master application key. Keepr uses it once to make a private bucket that keeps only the latest version of each file (so tidying up frees space), and a key that works only in that bucket. It doesn't keep the master key."}{" "}
          In Backblaze it's under Application Keys › Master Application Key. Making a new one stops the old one working.
        </>
      }
      link={{ label: "Open Application Keys in Backblaze", url: "https://secure.backblaze.com/app_keys.htm" }}
      fields={
        <>
          <label className="field">
            <span>Master key ID</span>
            <input className="input mono" value={keyId} onChange={(e) => setKeyId(e.target.value)} />
          </label>
          <label className="field">
            <span>Master application key</span>
            <input className="input" type="password" value={key} onChange={(e) => setKey(e.target.value)} />
          </label>
        </>
      }
      haveKey={!!(keyId.trim() && key.trim())}
      none="There are no buckets in this account."
      making={(b) => (mode === "source" ? `Making a read-only key for ${b}…` : "Making the bucket and its key…")}
      list={() => api.b2Buckets(keyId, key)}
      setup={async (b) => {
        const m = await api.b2SetupRun(keyId, key, b, mode);
        setKey("");
        return m;
      }}
      onMade={onMade}
    />
  );
}
