// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What grew on the size map: each folder's growth since the snapshot a week before
// (src-tauri/src/browse.rs), and the folders it landed in, which the map outlines and tags.
// Tidemark's growth layer (its history.rs `landed`), on a map's folders.

import type { MapDir } from "./api";
import { bytes, longDate } from "./format";

/** "+1.2 GB", "−300 MB". */
export function delta(n: number): string {
  return `${n < 0 ? "−" : "+"}${bytes(Math.abs(n))}`;
}

/** "this week", or "since 3 October" when the map isn't compared with the week before. */
export function sinceWhen(top: MapDir): string {
  return top.weekly ? "this week" : `since ${longDate(top.since)}`;
}

/**
 * Where growth landed, by path: a folder is left out when most of its growth is in one folder
 * below it (so the map tags Documents/Projects/big rather than every folder above it too), or
 * when the tagged folders below it already account for it. Growth smaller than `floor` isn't
 * tagged: 10 MB, or 0.5% of the map's top folder.
 */
export function landed(top: MapDir): Map<string, number> {
  const floor = Math.max(10e6, top.size * 0.005);
  // Every folder below the top with its growth, and the folders below it.
  type F = { d: MapDir; grew: number; below: F[]; depth: number };
  const walk = (d: MapDir, depth: number): F[] =>
    d.kids.map((k) => {
      const below = walk(k, depth + 1);
      return { d: k, grew: k.grew ?? 0, below, depth };
    });
  const flat: F[] = [];
  const all = (fs: F[]) => fs.forEach((f) => (flat.push(f), all(f.below)));
  all(walk(top, 1));
  const under = (f: F): F[] => f.below.flatMap((b) => [b, ...under(b)]);
  const cand = flat
    .filter((f) => Math.abs(f.grew) >= floor)
    .filter((f) => !under(f).some((b) => Math.sign(b.grew) === Math.sign(f.grew) && Math.abs(b.grew) >= 0.6 * Math.abs(f.grew)));
  // Deepest first, so a folder is judged against what is already kept below it.
  cand.sort((a, b) => b.depth - a.depth);
  const keep: F[] = [];
  for (const f of cand) {
    const inside = new Set(under(f));
    const kept = keep.filter((k) => inside.has(k));
    // The kept folders nearest it: none of the others is between it and them.
    const nearest = kept.filter((k) => !kept.some((o) => o !== k && under(o).includes(k)));
    const covered = nearest.reduce((n, k) => n + k.grew, 0);
    if (covered !== 0 && Math.abs(f.grew - covered) < Math.max(0.3 * Math.abs(f.grew), floor)) continue;
    keep.push(f);
  }
  return new Map(keep.map((f) => [f.d.path, f.grew]));
}
