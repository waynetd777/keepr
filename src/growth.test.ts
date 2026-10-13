// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import type { MapDir } from "./api";
import { landed } from "./growth";

const GB = 1e9;
const dir = (path: string, size: number, grew: number, kids: MapDir[] = []): MapDir => ({
  name: path.split("/").pop()!,
  path,
  size,
  files: 1,
  depth: path.split("/").length,
  looseFiles: 0,
  looseSize: 0,
  more: 0,
  moreSize: 0,
  read: true,
  grew,
  kids,
});

describe("landed", () => {
  it("tags the folder growth is concentrated in, not every folder above it", () => {
    const top = dir("", 20 * GB, 3 * GB, [dir("a", 10 * GB, 3 * GB, [dir("a/b", 8 * GB, 2.9 * GB, [dir("a/b/c", 5 * GB, 2.8 * GB)])])]);
    expect([...landed(top).keys()]).toEqual(["a/b/c"]);
  });

  it("tags each folder where it's split, and the parent only for what they don't cover", () => {
    const split = dir("", 20 * GB, 2 * GB, [dir("a", 10 * GB, 2 * GB, [dir("a/x", 4 * GB, 1 * GB), dir("a/y", 4 * GB, 1 * GB)])]);
    expect([...landed(split).keys()].sort()).toEqual(["a/x", "a/y"]);
    const more = dir("", 20 * GB, 5 * GB, [dir("a", 10 * GB, 5 * GB, [dir("a/x", 4 * GB, 1 * GB), dir("a/y", 4 * GB, 1 * GB)])]);
    expect([...landed(more).keys()].sort()).toEqual(["a", "a/x", "a/y"]);
  });

  it("leaves out small changes and folders with nothing to compare", () => {
    const top = dir("", 20 * GB, 0, [dir("a", 10 * GB, 5e6), { ...dir("b", 5 * GB, 0), grew: null }]);
    expect(landed(top).size).toBe(0);
  });
});
