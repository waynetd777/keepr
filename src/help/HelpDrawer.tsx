// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The help drawer: `?`, the toolbar's ? button or Help › Keepr Help (⌘?; on Windows, which has no
// Help menu, Ctrl+? here and the notification-area panel's Keepr Help) opens it on the right, on
// the current screen's topic. Under the search box, Getting started (the guides, one step at a
// time) and a picker of every topic; Keyboard shortcuts is one of them, opened at the screen's
// keys. One section is open at a time. It stays open while you follow a link into the app, so a
// guide can be walked through; Esc or × closes it.

import { openUrl } from "@tauri-apps/plugin-opener";
import { ReactNode, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import ReactMarkdown, { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { on } from "../api";
import { useApp, type Screen } from "../context";
import { Icon } from "../icons";
import { ClearButton } from "../ui";
import { isMac } from "../platform";
import { appLink, GUIDES, helpUrl, HelpSection, HelpTopic, keyRows, searchHelp, TOPICS, topicFor } from ".";

/** What the drawer shows: a topic (the screen's when none is named), the list of guides (`start`),
 *  or a guide at a step. */
export type HelpView = { topic?: string; guide?: string; step?: number; start?: boolean };

let view: HelpView | null = null;
const subs = new Set<() => void>();
const set = (v: HelpView | null) => {
  view = v;
  subs.forEach((f) => f());
};
const useView = () =>
  useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    () => view,
  );

export const openHelp = (v: HelpView = {}) => set(v);
export const toggleHelp = () => set(view ? null : {});
export const closeHelp = () => view && set(null);
/** Whether the drawer is open: Ask, which opens where it does, closes when it opens. */
export const useHelpOpen = () => useView() !== null;

/** The guides finished, kept in this computer's webview: only a tick in the list rides on it. */
const DONE_KEY = "keepr.helpGuides";
function doneGuides(): Record<string, boolean> {
  try {
    return JSON.parse(localStorage.getItem(DONE_KEY) ?? "{}");
  } catch {
    return {};
  }
}
function markDone(id: string) {
  try {
    localStorage.setItem(DONE_KEY, JSON.stringify({ ...doneGuides(), [id]: true }));
  } catch {
    // Without storage the guide just isn't ticked.
  }
}

/** The keyboard topic's section to open on a screen; Anywhere for the rest. */
const KEY_SECTION: Partial<Record<Screen["name"], string>> = {
  restore: "Restore",
  sizemap: "Size Map",
};

function HelpText({ text }: { text: string }) {
  const { go } = useApp();
  const components = useMemo<Components>(
    () => ({
      a: ({ href = "", children }) => {
        const to = appLink(href);
        const topic = /^help:([\w-]+)$/.exec(href)?.[1];
        return (
          <a
            href={href}
            onClick={(e) => {
              e.preventDefault();
              if (topic) openHelp({ topic });
              else if (to) {
                go(to);
                // Already on Destinations, the screen doesn't remount to see `add`.
                if (to.name === "destinations" && to.add) window.dispatchEvent(new Event("keepr:add-destination"));
              } else if (/^https?:/.test(href)) void openUrl(href);
            }}
          >
            {children}
          </a>
        );
      },
    }),
    [go],
  );
  return (
    <div className="help-text">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components} urlTransform={helpUrl}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

/** With `onToggle`, the drawer decides what is open (one section at a time), and a click on the
 *  title asks it. */
function Section({ s, open, onToggle, keys }: { s: HelpSection; open?: boolean; onToggle?: () => void; keys?: boolean }) {
  // A keyboard section's `- `⌘K` — Find a file` lines, as rows with the keys at the right.
  const rows = keys ? keyRows(s.body) : [];
  return (
    <details className="help-sec" open={open}>
      <summary
        onClick={
          onToggle &&
          ((e) => {
            e.preventDefault();
            onToggle();
          })
        }
      >
        <Icon name="forward" size={13} stroke={2} className="help-chev" />
        {s.title}
      </summary>
      {rows.length ? (
        <div className="help-keys">
          {rows.map(([k, what]) => (
            <div key={k + what} className="help-key">
              <span>{what}</span>
              <span>
                {k.split("  ").map((x) => (
                  <span key={x} className="kbd">
                    {x}
                  </span>
                ))}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <HelpText text={s.body} />
      )}
    </details>
  );
}

function GuideList() {
  const done = doneGuides();
  return (
    <section className="help-block">
      <HelpText text="Each guide walks you through part of Keepr a step at a time, with links that take you to the screen it's about while the help stays open." />
      <div className="help-guides">
        {GUIDES.map((g) => (
          <button
            key={g.id}
            className="help-guide"
            title={`Walk through “${g.title}”, a step at a time`}
            onClick={() => openHelp({ guide: g.id, step: 0 })}
          >
            <span className="help-guide-t">{g.title}</span>
            {done[g.id] ? (
              <span className="chip help-done">
                <Icon name="check" size={12} stroke={2.4} />
                Done
              </span>
            ) : (
              <span className="faint small">{g.sections.length} steps</span>
            )}
          </button>
        ))}
      </div>
    </section>
  );
}

function Guide({ g, step }: { g: HelpTopic; step: number }) {
  const n = g.sections.length;
  const at = Math.min(Math.max(step, 0), n - 1);
  const s = g.sections[at];
  const last = at === n - 1;
  return (
    <div className="help-guidebox">
      <div className="faint small">
        Step {at + 1} of {n}
      </div>
      <h3 className="help-steptitle">{s.title}</h3>
      <HelpText text={s.body} />
      <div className="help-steps">
        <span title={at === 0 ? "This is the first step" : "Go back a step"}>
          <button className="btn" disabled={at === 0} onClick={() => openHelp({ guide: g.id, step: at - 1 })}>
            Back
          </button>
        </span>
        <span className="help-dots" aria-hidden="true">
          {g.sections.map((x, i) => (
            <i key={x.title} className={i === at ? "on" : ""} />
          ))}
        </span>
        <button
          className="btn primary"
          title={last ? "Finish this guide and mark it done" : "Go to the next step"}
          onClick={() => {
            if (!last) return openHelp({ guide: g.id, step: at + 1 });
            markDone(g.id);
            openHelp({ start: true });
          }}
        >
          {last ? "Done" : "Next"}
        </button>
      </div>
    </div>
  );
}

function Results({ q }: { q: string }) {
  const hits = searchHelp(q).slice(0, 30);
  if (!hits.length) return <p className="muted">Nothing in the help matches “{q}”.</p>;
  return (
    <div className="help-results">
      {hits.map((h) => (
        <div key={h.topic.id + h.section.title}>
          <button className="help-hit-topic" title={`Open the help for ${h.topic.title}`} onClick={() => openHelp({ topic: h.topic.id })}>
            {h.topic.title}
          </button>
          <Section s={h.section} open />
        </div>
      ))}
    </div>
  );
}

export function HelpDrawer() {
  const v = useView();
  const { screen } = useApp();
  const [q, setQ] = useState("");

  // `?` outside a text box opens and closes it; Esc closes it; Help › Keepr Help opens it.
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape" && view && !document.querySelector("[role=dialog]")) {
        set(null);
        return;
      }
      // Ctrl+? on Windows, where macOS's ⌘? comes from the Help menu: some layouts report the
      // key under Ctrl as "/".
      if (!isMac && e.ctrlKey && !e.altKey && (e.key === "?" || (e.shiftKey && e.key === "/"))) {
        toggleHelp();
        e.preventDefault();
        return;
      }
      if (e.key !== "?" || e.metaKey || e.ctrlKey || e.altKey) return;
      if ((e.target as HTMLElement)?.closest?.("input, textarea, select, [contenteditable]")) return;
      toggleHelp();
      e.preventDefault();
    };
    window.addEventListener("keydown", k);
    const un = on("help", () => openHelp({}));
    return () => {
      window.removeEventListener("keydown", k);
      un();
    };
  }, []);
  useEffect(() => setQ(""), [v?.topic, v?.guide, v?.start]);
  const guide = v?.guide ? GUIDES.find((g) => g.id === v.guide) : undefined;
  const topic = (v?.topic && TOPICS.find((t) => t.id === v.topic)) || topicFor(screen.name);
  // The one section open in the topic: opening another closes it. It starts as the topic's first,
  // or for Keyboard shortcuts the screen's keys, and again whenever the topic changes.
  const first = topic?.id === "keyboard" ? (KEY_SECTION[screen.name] ?? "Anywhere") : topic?.sections[0]?.title;
  const [opened, setOpened] = useState<{ topic?: string; title: string | null }>({ title: null });
  if (!v) return null;
  const query = q.trim();
  const title = guide ? guide.title : v.start ? "Getting started" : topic ? topic.title : "Help";
  const openTitle = opened.topic === topic?.id ? opened.title : first;

  let body: ReactNode;
  if (query) body = <Results q={query} />;
  else if (guide) body = <Guide g={guide} step={v.step ?? 0} />;
  else if (v.start) body = <GuideList />;
  else if (topic)
    body = (
      <section className="help-block">
        {topic.intro && <HelpText text={topic.intro} />}
        {topic.sections.map((s) => (
          <Section
            key={`${topic.id}:${s.title}`}
            s={s}
            open={s.title === openTitle}
            keys={topic.id === "keyboard"}
            onToggle={() => setOpened({ topic: topic.id, title: s.title === openTitle ? null : s.title })}
          />
        ))}
      </section>
    );

  return (
    <aside className="help-drawer" aria-label="Help">
      <div className="help-head">
        {(guide || v.topic || v.start) && (
          <button
            className="iconbtn"
            aria-label="Back to this screen's help"
            title="Back to this screen's help"
            onClick={() => openHelp({})}
          >
            <Icon name="back" stroke={2} />
          </button>
        )}
        <h2>
          {title}
          {!v.start && <span className="faint"> — {guide ? "getting started" : "help"}</span>}
        </h2>
        <button className="iconbtn" aria-label="Close help" title="Close help (Esc)" onClick={() => set(null)}>
          <Icon name="close" stroke={2} />
        </button>
      </div>
      <div className="search help-search">
        <Icon name="search" size={15} stroke={2} />
        <input aria-label="Search the help" placeholder="Search the help" value={q} onChange={(e) => setQ(e.target.value)} />
        {q && <ClearButton onClick={() => setQ("")} />}
      </div>
      <div className="help-nav">
        <button
          className={`btn${guide || v.start ? " on" : ""}`}
          title="Step-by-step guides to everything Keepr does"
          onClick={() => openHelp({ start: true })}
        >
          <Icon name="play" size={12} />
          Getting started
        </button>
        <select
          className="input help-pick"
          aria-label="Help topic"
          title="Open the help for another part of Keepr"
          value={guide || v.start ? "" : (topic?.id ?? "")}
          onChange={(e) => e.target.value && openHelp({ topic: e.target.value })}
        >
          <option value="" disabled>
            All topics
          </option>
          {TOPICS.filter((t) => t.kind === "screen").map((t) => (
            <option key={t.id} value={t.id} title={t.summary}>
              {t.title}
            </option>
          ))}
        </select>
      </div>
      <div className="help-body">{body}</div>
    </aside>
  );
}

/** The toolbar's ? button. */
export function HelpButton() {
  const open = useView();
  return (
    <button className={`iconbtn help-btn${open ? " on" : ""}`} aria-label="Help" title="Help for this screen ?" onClick={toggleHelp}>
      <Icon name="help" stroke={2} />
    </button>
  );
}
