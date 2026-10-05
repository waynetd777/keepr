// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What every screen shares: the overview, the configuration, the running job, where we are, and
// how to go somewhere else. Kept apart from App.tsx so hot reload can swap App in place.

import { createContext, useContext } from "react";
import type { Config, JobStatus, Overview as OverviewData } from "./api";

export type Screen =
  | { name: "overview" }
  | { name: "sizemap" }
  | { name: "plans"; plan?: string; isNew?: boolean }
  | { name: "restore"; plan?: string; query?: string; focus?: { snapshot: string; path: string }; view?: "files" | "map" }
  | { name: "search"; query: string }
  | { name: "activity" }
  | { name: "destinations"; add?: boolean }
  | { name: "settings" };

export type AppCtx = {
  ov: OverviewData | null;
  cfg: Config | null;
  job: JobStatus | null;
  home: string;
  screen: Screen;
  go: (s: Screen) => void;
  refresh: () => Promise<void>;
};

export const Ctx = createContext<AppCtx>(null as unknown as AppCtx);
export const useApp = () => useContext(Ctx);

export function statusDot(status: string): string {
  if (status === "running") return "dot spin";
  if (status === "waiting" || status === "stale") return "dot amber";
  if (status === "failed") return "dot red";
  if (status === "off" || status === "never") return "dot grey";
  return "dot";
}
