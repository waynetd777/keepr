// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A destination's connection and how full it is, as the Overview and Destinations show them.

import type { DestSummary } from "./api";

/** "Connected", "Connects when needed", or "Not connected" in amber. `lower` for mid-sentence
 *  use, as in the Overview's compact list. */
export function Connection({ d, lower }: { d: Pick<DestSummary, "connection">; lower?: boolean }) {
  const text = d.connection === "missing" ? "Not connected" : d.connection === "on demand" ? "Connects when needed" : "Connected";
  return (
    <span className="small" style={{ color: d.connection === "missing" ? "var(--amber)" : "var(--ink2)" }}>
      {lower && d.connection !== "missing" ? text.toLowerCase() : text}
    </span>
  );
}

/** How much of the destination's disk is used, when its size is known. */
export function SpaceMeter({ d, height }: { d: Pick<DestSummary, "free" | "total">; height?: number }) {
  if (d.total == null) return null;
  return (
    <div className="meter" style={height ? { height } : undefined}>
      <div style={{ width: `${Math.round((1 - (d.free ?? 0) / d.total) * 100)}%` }} />
    </div>
  );
}
