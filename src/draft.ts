// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A new plan not yet created, kept while Keepr runs so leaving its page doesn't lose it. Held in
// memory only: closing the window hides it and keeps this, quitting Keepr discards it. The
// password typed for it isn't kept.

import { useSyncExternalStore } from "react";
import type { Plan } from "./api";

let draft: Plan | null = null;
const listeners = new Set<() => void>();

export function setDraft(p: Plan | null) {
  draft = p;
  listeners.forEach((l) => l());
}

export function getDraft(): Plan | null {
  return draft;
}

export function useDraft(): Plan | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => draft,
  );
}

/** What the draft is called where it's mentioned: its name, or "A new plan". */
export function draftName(p: Plan): string {
  return p.name.trim() || "A new plan";
}

export const DRAFT_NOTE = "It's kept while Keepr is open, and discarded if you quit Keepr before creating it.";
