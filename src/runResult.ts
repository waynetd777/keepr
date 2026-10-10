// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// How a run's result is shown: one word for it and one colour, the same in Activity's history
// and the Overview's Recent list.

import type { Run } from "./api";

export type Tone = "ok" | "bad" | "plain" | "warn";

/** The tone of a result, as a .tag class: green, red, grey or amber. */
export function resultTone(result: Run["result"]): Tone {
  return result === "ok" ? "ok" : result === "failed" ? "bad" : result === "cancelled" ? "plain" : "warn";
}

/** The same tone as a text colour, for where a result is a word and not a tag. */
export const TONE_COLOR: Record<Tone, string> = {
  ok: "var(--accent-text)",
  bad: "var(--red)",
  plain: "var(--ink3)",
  warn: "var(--amber)",
};

/** "Complete", "Stopped", "Done, with problems"… or, `short`, a mark where a word would crowd the
 *  row: ✓ for done, ! for done with problems. */
export function resultLabel(r: Pick<Run, "result" | "kind">, short = false): string {
  switch (r.result) {
    case "ok":
      return short ? "✓" : r.kind === "check" ? "All intact" : "Complete";
    case "cancelled":
      return "Stopped";
    case "waiting":
      return "Waiting";
    case "warning":
      return short ? "!" : "Done, with problems";
    case "failed":
      return "Failed";
  }
}
