// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import { describe, expect, it } from "vitest";
import { appLink, GUIDES, helpUrl, KEYS, keyRows, parseTopic, searchHelp, TOPICS, topicFor } from ".";
import type { Screen } from "../context";

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

  it("has the getting-started guides, each with steps that link into the app", () => {
    expect(GUIDES.map((g) => g.title)).toEqual(["Start here", "Get a file back", "Keep an eye on it"]);
    for (const g of GUIDES) {
      expect(g.sections.length, g.id).toBeGreaterThanOrEqual(3);
      for (const s of g.sections) expect(s.body, `${g.id} › ${s.title}`).toMatch(/\]\(app:/);
    }
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
    expect(searchHelp("zzqx")).toEqual([]);
    expect(searchHelp("  ")).toEqual([]);
  });

  it("gives the shortcut list to the drawer", () => {
    expect(keyRows("- `⌘[` `⌘]` — Back, forward\nnot a row")).toEqual([["⌘[  ⌘]", "Back, forward"]]);
    expect(KEYS.map(([t]) => t)).toEqual(["Anywhere", "Restore", "Menu bar panel"]);
    expect(KEYS[0][1]).toContainEqual(["?", "Help for this screen"]);
  });
});
