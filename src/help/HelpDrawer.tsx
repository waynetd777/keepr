// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The help drawer: `?`, the toolbar's ? button or Help › Keepr Help (⌘?) opens it on the right, on
// the current screen's topic. Typing filters its sections and searches every topic; below them,
// the getting-started guides (one step at a time) and the screen's keys. It stays open while you
// follow a link into the app, so a guide can be walked through; Esc or × closes it.

import { openUrl } from "@tauri-apps/plugin-opener";
import { ReactNode, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import ReactMarkdown, { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { on } from "../api";
import { useApp, type Screen } from "../context";
import { Icon } from "../icons";
import { ClearButton } from "../ui";
import { appLink, GUIDES, helpUrl, HelpSection, HelpTopic, KEYS, searchHelp, TOPICS, topicFor } from ".";

/** What the drawer shows: a topic (the screen's when none is named), or a guide at a step. */
export type HelpView = { topic?: string; guide?: string; step?: number };

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

/** The guides finished, kept in this Mac's webview: only a tick in the list rides on it. */
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

/** The keyboard section for a screen, besides Anywhere. */
const KEY_SECTION: Partial<Record<Screen["name"], string>> = {
  restore: "Restore",
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

function Section({ s, open }: { s: HelpSection; open?: boolean }) {
  return (
    <details className="help-sec" open={open}>
      <summary>
        <Icon name="forward" size={13} stroke={2} className="help-chev" />
        {s.title}
      </summary>
      <HelpText text={s.body} />
    </details>
  );
}

function Keys({ screen }: { screen: Screen["name"] }) {
  const wanted = ["Anywhere", KEY_SECTION[screen]].filter(Boolean);
  return (
    <section className="help-block">
      <div className="help-blockhead">
        <span className="help-eyebrow">Keyboard</span>
        <button className="btn small" title="Show every keyboard shortcut" onClick={() => openHelp({ topic: "keyboard" })}>
          All shortcuts
        </button>
      </div>
      {KEYS.filter(([t]) => wanted.includes(t)).map(([t, rows]) => (
        <div key={t} className="help-keys">
          {wanted.length > 1 && <div className="faint small">{t}</div>}
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
      ))}
    </section>
  );
}

function GuideList() {
  const done = doneGuides();
  return (
    <section className="help-block">
      <span className="help-eyebrow">Getting started</span>
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
            openHelp({});
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
  useEffect(() => setQ(""), [v?.topic, v?.guide]);

  if (!v) return null;
  const guide = v.guide ? GUIDES.find((g) => g.id === v.guide) : undefined;
  const topic = (v.topic && TOPICS.find((t) => t.id === v.topic)) || topicFor(screen.name);
  const query = q.trim();
  const title = guide ? guide.title : topic ? topic.title : "Help";

  let body: ReactNode;
  if (query) body = <Results q={query} />;
  else if (guide) body = <Guide g={guide} step={v.step ?? 0} />;
  else
    body = (
      <>
        {topic && (
          <section className="help-block">
            {topic.intro && <HelpText text={topic.intro} />}
            {topic.sections.map((s, i) => (
              <Section key={`${topic.id}:${s.title}`} s={s} open={i === 0} />
            ))}
          </section>
        )}
        <GuideList />
        <Keys screen={screen.name} />
        <section className="help-block">
          <span className="help-eyebrow">All topics</span>
          <div className="help-topics">
            {TOPICS.filter((t) => t.kind === "screen" && t.id !== topic?.id).map((t) => (
              <button key={t.id} className="chip" title={t.summary} onClick={() => openHelp({ topic: t.id })}>
                {t.title}
              </button>
            ))}
          </div>
        </section>
      </>
    );

  return (
    <aside className="help-drawer" aria-label="Help">
      <div className="help-head">
        {(guide || v.topic) && (
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
          <span className="faint"> — {guide ? "getting started" : "help"}</span>
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
