// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What setting up an S3 service from a key the person pastes has in common, whichever service
// it is: the card, the bucket name for a new destination, listing the buckets for a source, and
// the picker to choose one. Also the endpoint each service's form builds.

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState, type ReactNode } from "react";
import { api, type AwsMade } from "./api";
import { Icon } from "./icons";

export type Service = "aws" | "b2" | "r2" | "other";

/** The S3 service an endpoint belongs to, and what the form needs to rebuild it. */
export function serviceOf(endpoint: string): { service: Service; r2Account: string } {
  const host = endpoint.replace(/^[a-z]+:\/\//, "").split(/[/:]/)[0];
  if (!endpoint || host.endsWith(".amazonaws.com")) return { service: "aws", r2Account: "" };
  if (host.endsWith(".backblazeb2.com")) return { service: "b2", r2Account: "" };
  if (host.endsWith(".r2.cloudflarestorage.com")) return { service: "r2", r2Account: host.split(".")[0] };
  return { service: "other", r2Account: "" };
}

/** The S3 endpoint for a service: built from the region or R2 account, or as typed for "other". */
export function s3EndpointFor(service: Service, region: string, r2Account: string, endpoint: string): string {
  if (service === "aws") return `https://s3.${region.trim()}.amazonaws.com`;
  if (service === "b2") return `https://s3.${region.trim()}.backblazeb2.com`;
  if (service === "r2") return `https://${r2Account.trim()}.r2.cloudflarestorage.com`;
  return endpoint.trim();
}

/** The buckets an account has, one button each, to choose the one to back up. */
export function BucketPicker({
  buckets,
  busy,
  none = "There are no buckets in this account.",
  onPick,
}: {
  buckets: string[];
  busy: boolean;
  none?: string;
  onPick: (bucket: string) => void;
}) {
  return (
    <div className="col" style={{ gap: 6 }}>
      <span className="small muted">{buckets.length ? "Choose the bucket to back up:" : none}</span>
      <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
        {buckets.map((b) => (
          <button key={b} className="btn small" disabled={busy} onClick={() => onPick(b)}>
            <Icon name="bucket" size={13} />
            {b}
          </button>
        ))}
      </div>
    </div>
  );
}

/** A service's setup card. The service gives its own words, key fields and calls; this does the
 *  rest. For a destination it makes a new bucket; for a source it lists the buckets to choose from. */
export function KeySetup({
  mode,
  intro,
  link,
  fields,
  haveKey,
  none,
  making,
  list,
  setup,
  onMade,
}: {
  mode: "destination" | "source";
  intro: ReactNode;
  link: { label: string; url: string };
  fields: ReactNode;
  haveKey: boolean;
  none: string;
  making: (bucket: string) => string;
  list: () => Promise<string[]>;
  setup: (bucket: string) => Promise<AwsMade>;
  onMade: (m: AwsMade) => void;
}) {
  const [bucket, setBucket] = useState("");
  const [buckets, setBuckets] = useState<string[] | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    if (mode !== "destination") return;
    // The suggested name only fills an empty field: a reply that comes late never replaces a
    // name already typed. Without a suggestion the field is just left empty.
    let live = true;
    api.awsSetupInfo().then(
      (i) => live && setBucket((b) => b || i.bucket),
      () => {},
    );
    return () => {
      live = false;
    };
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
  const made = (b: string) => run(making(b), async () => onMade(await setup(b)));
  return (
    <div className="card col" style={{ padding: 14, gap: 10, background: "var(--sunk)" }}>
      <span style={{ fontWeight: 600 }}>
        {mode === "source" ? "Let Keepr set up read-only access" : "New to this? Let Keepr set it up"}
      </span>
      <span className="small muted" style={{ lineHeight: 1.5 }}>
        {intro}
      </span>
      <div className="row" style={{ gap: 8 }}>
        <button className="btn small" onClick={() => openUrl(link.url)}>
          {link.label}
        </button>
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "10px 12px" }}>
        {fields}
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
            onClick={() => run("Asking for the buckets…", async () => setBuckets(await list()))}
          >
            List my buckets
          </button>
        )}
        <span className="small muted grow">{busy}</span>
      </div>
      {buckets && <BucketPicker buckets={buckets} busy={!!busy} none={none} onPick={made} />}
      {error && (
        <span className="small" style={{ color: "var(--red)" }}>
          {error}
        </span>
      )}
    </div>
  );
}
