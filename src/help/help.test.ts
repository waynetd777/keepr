// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { appLink, FILES, forPlatform, GUIDES, helpUrl, KEYS, keyRows, parseTopic, searchHelp, TOPICS, topicFor } from ".";
import type { Screen } from "../context";
import { isMac } from "../platform";

const SCREENS: Screen["name"][] = ["overview", "sizemap", "plans", "restore", "search", "activity", "destinations", "settings"];

describe("help topics", () => {
  it("parses a file", () => {
    const t = parseTopic(
      "x",
      "---\ntitle: Restore\nkind: screen\nscreens: [restore, search]\norder: 2\nsummary: S.\n---\nIntro.\n\n## Keys\nJ.\n",
    );
    expect(t).toMatchObject({
      id: "x",
      title: "Restore",
      kind: "screen",
      screens: ["restore", "search"],
      order: 2,
      summary: "S.",
      intro: "Intro.",
    });
    expect(t.sections).toEqual([{ title: "Keys", body: "J." }]);
  });

  it("has a topic for every screen", () => {
    for (const s of SCREENS) expect(topicFor(s)?.screens, s).toContain(s);
  });

  it("has getting-started guides that link into every screen", () => {
    expect(GUIDES[0].id).toBe("guide-start");
    expect(GUIDES.length).toBeGreaterThanOrEqual(10);
    for (const g of GUIDES) {
      expect(g.sections.length, g.id).toBeGreaterThanOrEqual(3);
      for (const s of g.sections) expect(s.body, `${g.id} › ${s.title}`).toMatch(/\]\(app:/);
    }
    const reached = new Set(
      GUIDES.flatMap((g) =>
        [
          ...g.sections
            .map((s) => s.body)
            .join("\n")
            .matchAll(/\]\(app:([a-z]+)/g),
        ].map((m) => m[1]),
      ),
    );
    // Search opens from the toolbar's box; every other screen has a guide that takes you there.
    for (const s of SCREENS.filter((x) => x !== "search")) expect(reached, s).toContain(s);
  });

  it("keeps to the house rules", () => {
    for (const t of TOPICS) {
      const all = [t.title, t.summary, t.intro, ...t.sections.flatMap((s) => [s.title, s.body])].join("\n");
      expect(t.summary, t.id).not.toBe("");
      expect(t.sections.length, t.id).toBeGreaterThan(0);
      // Emoji only inside code, where they'd show a file's contents.
      expect(all.replace(/`[^`]*`/g, ""), t.id).not.toMatch(/\p{Emoji_Presentation}/u);
      for (const s of t.screens) expect(SCREENS, `${t.id}: ${s}`).toContain(s);
      for (const m of all.matchAll(/\]\(help:([^)]*)\)/g))
        expect(
          TOPICS.map((x) => x.id),
          `${t.id}: help:${m[1]}`,
        ).toContain(m[1]);
      for (const m of all.matchAll(/\]\((app:[^)]*)\)/g)) expect(appLink(m[1]), `${t.id}: ${m[1]}`).not.toBeNull();
    }
  });

  it("reads app links", () => {
    expect(appLink("app:restore")).toEqual({ name: "restore" });
    expect(appLink("app:plans/new")).toEqual({ name: "plans", isNew: true });
    expect(appLink("app:destinations/add")).toEqual({ name: "destinations", add: true });
    expect(appLink("app:search")).toBeNull();
    expect(appLink("app:restore/new")).toBeNull();
    expect(appLink("https://example.com")).toBeNull();
  });

  it("keeps app and help links through the markdown renderer", () => {
    expect(helpUrl("app:plans/new")).toBe("app:plans/new");
    expect(helpUrl("help:restore")).toBe("help:restore");
    expect(helpUrl("https://example.com")).toBe("https://example.com");
    expect(helpUrl("javascript:alert(1)")).toBe("");
  });

  it("finds sections by every word typed", () => {
    const hits = searchHelp("recovery key");
    expect(hits.length).toBeGreaterThan(0);
    expect(hits.every((h) => /recover/i.test(h.topic.title + h.topic.summary + h.section.title + h.section.body))).toBe(true);
    expect(searchHelp("B2").length).toBeGreaterThan(0);
    expect(searchHelp("zzqx")).toEqual([]);
    expect(searchHelp("  ")).toEqual([]);
  });

  it("gives the shortcut list to the drawer", () => {
    expect(keyRows("- `⌘[` `⌘]` — Back, forward\nnot a row")).toEqual([["⌘[  ⌘]", "Back, forward"]]);
    expect(KEYS.map(([t]) => t)).toEqual(["Anywhere", "Restore", "Size Map", isMac ? "Menu bar panel" : "Notification area panel"]);
    expect(KEYS[0][1]).toContainEqual(["?", "Help for this screen"]);
  });

  it("closes every platform part, so the help and Ask drop only what they should", () => {
    for (const [name, page] of Object.entries(FILES))
      for (const m of ["mac", "windows", "github"]) {
        const opens = page.split(`<!-- ${m} -->`).length - 1;
        const closes = page.split(`<!-- /${m} -->`).length - 1;
        expect(closes, `${name}: <!-- ${m} -->`).toBe(opens);
      }
  });

  it("keeps this platform's parts", () => {
    const md =
      "Show in <!-- mac -->Finder<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->File Explorer<!-- /windows -->.\n\n<!-- windows -->\nOnly Windows.\n<!-- /windows -->\n\nEnd.";
    expect(forPlatform(md, true)).toBe("Show in Finder.\n\nEnd.");
    expect(forPlatform(md, false)).toBe("Show in File Explorer.\n\nOnly Windows.\n\nEnd.");
  });

  it("reads as a whole guide on both platforms", () => {
    for (const mac of [true, false])
      for (const [name, page] of Object.entries(FILES)) {
        const t = parseTopic(name, forPlatform(page, mac));
        const all = [t.title, t.summary, t.intro, ...t.sections.flatMap((s) => [s.title, s.body])].join("\n");
        expect(all, `${name} (${mac ? "mac" : "windows"})`).not.toMatch(/<!--/);
        expect(t.sections.length, name).toBeGreaterThan(0);
        if (t.kind === "guide") for (const s of t.sections) expect(s.body, `${name} › ${s.title}`).toMatch(/\]\(app:/);
      }
  });
});
