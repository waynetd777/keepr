// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Ask: a chat about the backups, answered by an AI CLI already signed in on this Mac (as
// Tidemark's Ask is; src-tauri/src/assistant/). The first message carries what Keepr knows
// (src-tauri/src/ask.rs: the plans, their biggest folders and growth, the space stored, the recent
// runs), as the help's Ask page describes it; a follow-up resumes the CLI's session. An Ask
// button elsewhere (a size map's folder, a run in Activity) opens it with a question and its own
// details. A change an answer proposes is shown, checked and applied in AskChange.tsx.

import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { GROUP_NAME, loadModel, modelGroups, pickModel, providerOf, saveModel, useAssistant, type Provider } from "./assistant";
import { AskChange, splitChanges } from "./AskChange";
import { AskIcon } from "./AskButton";
import { useApp } from "./context";
import { closeHelp, useHelpOpen } from "./help/HelpDrawer";
import { Icon } from "./icons";
import { useRequests } from "./ui";
import { mdToHtml } from "./md";
import { bytes } from "./format";

const SUGGESTIONS = [
  "What takes the most space, and does all of it need backing up?",
  "Why did my backups grow this week?",
  "Are my plans backing up anything that tools can rebuild?",
  "Do my plans keep a sensible number of versions?",
];

interface Message {
  role: "user" | "assistant";
  text: string;
  error?: boolean;
  /** Done: its suggested changes can be shown. */
  done?: boolean;
  /** What the question came with: "the run at 10:30". */
  about?: string;
}

/** Details a button hands to Ask with its question: a folder, a run. */
export interface AskDetail {
  /** A few words naming it, shown above the box and on the question. */
  label: string;
  /** What the assistant is told about it. */
  text: string;
}

/** A question handed in from elsewhere, typed into the box to edit or send. */
export interface AskSeed {
  text: string;
  detail?: AskDetail;
  /** The plan it's about, described first. */
  plan?: string;
}

// Open or closed, and the last seed, kept outside React so any screen can open it.
let shown = false;
let seedNow: (AskSeed & { n: number }) | null = null;
const subs = new Set<() => void>();
const publish = () => subs.forEach((f) => f());
export function openAsk(seed?: AskSeed) {
  shown = true;
  if (seed) seedNow = { ...seed, n: Date.now() };
  closeHelp();
  publish();
}
export function closeAsk() {
  shown = false;
  publish();
}
export const toggleAsk = () => (shown ? closeAsk() : openAsk());
const useAskState = () =>
  useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    () => (shown ? (seedNow ? seedNow.n : 1) : 0),
  );

// The toolbar's button, which takes the focus back when the panel closes.
let toolbarButton: HTMLButtonElement | null = null;

/** The toolbar's Ask button: green while the panel is open, as Help's is. */
export function AskToolbarButton() {
  const open = useAskState() !== 0;
  return (
    <button
      ref={(el) => {
        toolbarButton = el;
      }}
      className={`btn askbtn${open ? " on" : ""}`}
      aria-pressed={open}
      onClick={toggleAsk}
      title="Ask an AI about your backups ⌘J"
    >
      <AskIcon />
      Ask
    </button>
  );
}

/** One question put to the AI tool: its events carry `id`, so what comes for an earlier one (a
 * stopped answer's last words, its CLI's ask-done) can't land on this one's answer. */
interface Asked {
  id: string;
  /** The chat it was asked in. */
  chat: string;
  stopped: boolean;
  /** Waiting on the figures, which closing the panel then leaves to finish. */
  figures: boolean;
}

/** An answer's text split from its keepr-change blocks, worked out once per text. */
function Answer({ m, i, copied, copy }: { m: Message; i: number; copied: number | null; copy: (i: number, text: string) => void }) {
  const { text, changes } = useMemo(() => splitChanges(m.text, !m.done && !m.error), [m.text, m.done, m.error]);
  return (
    <div className="ask-turn">
      <div className={`ask-a${m.error ? " error" : ""}`} dangerouslySetInnerHTML={{ __html: mdToHtml(text) }} />
      {m.done && changes.map((c, j) => <AskChange key={j} raw={c.raw} bad={c.bad} />)}
      {!m.error && (
        <button type="button" className="btn small ask-copy" title="Copy this answer" onClick={() => copy(i, text)}>
          {copied === i ? "Copied" : "Copy"}
        </button>
      )}
    </div>
  );
}

/** Whether an event for question `id` counts: it's for `a`, which isn't stopped. A Stop before its
 * CLI had started may not have reached it, so a stopped one is stopped again as it speaks. */
function live(a: Asked | null, id: string) {
  if (!a || a.id !== id) return false;
  if (a.stopped) void api.askCancel(id);
  return !a.stopped;
}

const uid = () => (typeof crypto.randomUUID === "function" ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`);

export default function AskPanel() {
  const state = useAskState();
  const open = state !== 0;
  const helpOpen = useHelpOpen();
  const { cfg, go } = useApp();
  const off = cfg?.settings.ask === false;
  const [preferred, setModel] = useState(loadModel);
  const assistant = useAssistant();
  const model = pickModel(preferred, assistant.models);
  const [copied, setCopied] = useState<number | null>(null);
  const [chatId, setChatId] = useState(uid);
  const [messages, setMessages] = useState<Message[]>([]);
  const [session, setSession] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [q, setQ] = useState("");
  const [detail, setDetail] = useState<AskDetail | null>(null);
  const [plan, setPlan] = useState<string | undefined>();
  const sessionTool = useRef<Provider | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const panel = useRef<HTMLElement>(null);
  const composedAt = useRef(0);
  const scroller = useRef<HTMLDivElement>(null);
  const chatRef = useRef(chatId);
  chatRef.current = chatId;
  // The question being answered (or last answered): only its events count, and none once stopped.
  const asked = useRef<Asked | null>(null);

  // Help opening closes Ask: they share the right edge.
  useEffect(() => {
    if (helpOpen && shown) closeAsk();
  }, [helpOpen]);

  useEffect(() => {
    const chunk = listen<{ chatId: string; text: string }>("ask-chunk", (e) => {
      if (!live(asked.current, e.payload.chatId)) return;
      setMessages((m) => {
        const last = m[m.length - 1];
        if (!last || last.role !== "assistant") return m;
        return [...m.slice(0, -1), { ...last, text: last.text + e.payload.text }];
      });
    });
    const done = listen<{ chatId: string; sessionId: string | null; text: string; error: string | null }>("ask-done", (e) => {
      const d = e.payload;
      const a = asked.current;
      if (!a || a.id !== d.chatId) return;
      // Stopped: the answer so far (or "Stopped.") stays, without the tool's complaint at being
      // stopped; its session still serves a follow-up in the same chat.
      if (a.stopped) {
        if (d.sessionId && a.chat === chatRef.current) setSession(d.sessionId);
        return;
      }
      setMessages((m) => {
        const last = m[m.length - 1];
        if (!last || last.role !== "assistant") return m;
        const fin: Message = d.error
          ? { role: "assistant", text: last.text ? `${last.text}\n\n${d.error}` : d.error, error: true }
          : { ...last, text: last.text || d.text, done: true };
        return [...m.slice(0, -1), fin];
      });
      if (d.sessionId) setSession(d.sessionId);
      setBusy(false);
    });
    return () => {
      void chunk.then((un) => un());
      void done.then((un) => un());
    };
  }, []);

  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  useEffect(() => {
    if (open) box.current?.focus();
  }, [open]);

  // What Keepr is reading for the question's figures (src-tauri/src/ask.rs), while it does.
  const [reading, setReading] = useState<string | null>(null);
  // What the AI tool is doing before its answer starts (src-tauri/src/assistant/), and since when
  // the question has been waiting, ticking each second.
  const [doing, setDoing] = useState<string | null>(null);
  const [since, setSince] = useState(0);
  const [, tick] = useState(0);
  useEffect(() => {
    const un = listen<{ chatId: string; text: string }>("ask-status", (e) => {
      if (live(asked.current, e.payload.chatId)) setDoing(e.payload.text);
    });
    return () => void un.then((f) => f());
  }, []);
  useEffect(() => {
    if (!busy) return;
    const id = window.setInterval(() => tick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [busy]);
  // Its events carry no id, so they count only while a read is in flight: a late one can't put
  // "Reading…" back over what the AI tool is doing.
  const inFlight = useRef<object | null>(null);
  useEffect(() => {
    const un = listen<string>("ask-progress", (e) => {
      if (inFlight.current) setReading(e.payload);
    });
    return () => void un.then((f) => f());
  }, []);
  // The figures, read as Ask opens so the first question needn't wait for them; a question within
  // two minutes uses them, a later one reads them again, as does one after Ask or a plan's "leave
  // out of Ask" changed (`fp`).
  const req = useRequests();
  const fp = cfg ? JSON.stringify([cfg.settings.ask, cfg.plans.map((p) => [p.id, !!p.askExclude])]) : "";
  const figures = useRef<{ plan?: string; fp: string; at: number; text: Promise<string> } | null>(null);
  const figuresFor = (p?: string) => {
    const f = figures.current;
    if (f && f.plan === p && f.fp === fp && Date.now() - f.at < 120_000) return f.text;
    const text = req.run((t) => api.askContext(p, t), "figures");
    const mine = { plan: p, fp, at: Date.now(), text };
    figures.current = mine;
    inFlight.current = mine;
    const settle = () => {
      if (inFlight.current === mine) inFlight.current = null;
    };
    void text.then(settle, settle);
    // A read replaced by a newer one leaves the newer one's reuse and progress alone.
    text.then(
      () => figures.current === mine && setReading(null),
      () => {
        if (figures.current !== mine) return;
        setReading(null);
        figures.current = null;
      },
    );
    return text;
  };
  // Not while a question waits on a read: a new one would stop it, and the question with it.
  useEffect(() => {
    if (open && !off && assistant.available && !session && !asked.current?.figures) void figuresFor(plan).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, off, assistant.available, plan, fp]);
  // Closing the panel (it's hidden, not gone) stops a read nobody is waiting on, and gives the
  // focus back to the toolbar's Ask button.
  const wasOpen = useRef(open);
  useEffect(() => {
    if (wasOpen.current && !open) {
      if (!asked.current?.figures) {
        req.stop("figures");
        figures.current = null;
        inFlight.current = null;
        setReading(null);
      }
      const el = document.activeElement;
      if (!helpOpen && (!el || el === document.body || panel.current?.contains(el))) toolbarButton?.focus();
    }
    wasOpen.current = open;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // A new seed is typed into the box as it arrives, ready to finish or send.
  const [seen, setSeen] = useState(0);
  if (state > 1 && state !== seen && seedNow) {
    setSeen(state);
    setQ(seedNow.text);
    setDetail(seedNow.detail ?? null);
    setPlan(seedNow.plan);
  }
  useEffect(() => {
    if (state <= 1) return;
    const el = box.current;
    if (el) {
      el.focus();
      window.setTimeout(() => el.setSelectionRange(el.value.length, el.value.length), 0);
    }
  }, [state]);

  // Stop: while the figures are still being read there's no AI to stop yet, so the reading stops
  // and the question isn't sent when they'd have come.
  const stop = () => {
    const a = asked.current;
    if (a) {
      a.stopped = true;
      void api.askCancel(a.id);
    }
    req.stop("figures");
    inFlight.current = null;
    setMessages((m) => {
      const last = m[m.length - 1];
      return last?.role === "assistant" && !last.text ? [...m.slice(0, -1), { role: "assistant", text: "Stopped.", error: true }] : m;
    });
    setReading(null);
    setBusy(false);
  };

  const send = async (question: string) => {
    const text = question.trim();
    if (!text || busy || off) return;
    const me: Asked = { id: uid(), chat: chatId, stopped: false, figures: false };
    asked.current = me;
    const tool = providerOf(model);
    let resume = session;
    if (sessionTool.current !== tool) {
      // Another tool has seen nothing of this chat, so Keepr's figures go again.
      resume = null;
      setSession(null);
      sessionTool.current = tool;
    }
    const about = detail ? `About this — ${detail.label}:\n${detail.text}\n\n` : "";
    setQ("");
    setDetail(null);
    setBusy(true);
    setDoing(null);
    setSince(Date.now());
    setMessages((m) => [...m, { role: "user", text, about: detail?.label }, { role: "assistant", text: "" }]);
    try {
      let prompt = `${about}Question: ${text}`;
      if (!resume) {
        me.figures = true;
        const ctx = await figuresFor(plan).finally(() => {
          me.figures = false;
          if (asked.current === me) setReading(null);
        });
        // Stopped, or a new chat begun, while they were read.
        if (me.stopped) return;
        prompt = `${ctx}\n\n${prompt}`;
        setDoing(`Sending ${bytes(new TextEncoder().encode(prompt).length)} of figures to ${GROUP_NAME[tool]}`);
      }
      // Its events carry the question's own id; the chat carries on through `resume`.
      await api.ask(me.id, prompt, model, resume);
      // A Stop while it started may have come before its CLI could be stopped.
      if (me.stopped) void api.askCancel(me.id);
    } catch (e) {
      if (me.stopped) return;
      setMessages((m) => [...m.slice(0, -1), { role: "assistant", text: String(e), error: true }]);
      setBusy(false);
    }
  };

  const fresh = () => {
    const a = asked.current;
    if (a && !a.stopped) {
      a.stopped = true;
      void api.askCancel(a.id);
    }
    req.stop("figures");
    inFlight.current = null;
    setReading(null);
    setChatId(uid());
    setMessages([]);
    setSession(null);
    setPlan(undefined);
    sessionTool.current = null;
    setBusy(false);
    box.current?.focus();
  };

  const copy = async (i: number, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(i);
      window.setTimeout(() => setCopied((c) => (c === i ? null : c)), 1500);
    } catch (e) {
      console.error(e);
    }
  };

  const toolName = GROUP_NAME[providerOf(model)];

  return (
    <aside
      ref={panel}
      className="ask"
      hidden={!open}
      aria-label="Ask about your backups"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          closeAsk();
        }
      }}
    >
      <div className="ask-head">
        <b>Ask</b>
        {assistant.models.length > 0 && (
          <select
            className="btn small"
            aria-label="Model"
            title="Which model answers"
            value={model}
            onChange={(e) => {
              saveModel(e.target.value);
              setModel(e.target.value);
            }}
          >
            {modelGroups(assistant.models).map((g) => (
              <optgroup key={g.provider} label={g.name}>
                {g.models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        )}
        <div className="grow" />
        {messages.length > 0 && (
          <button className="btn small" onClick={fresh} title="Start a new chat">
            New chat
          </button>
        )}
        <button className="btn small" onClick={closeAsk} aria-label="Close" title="Close (Esc)">
          ✕
        </button>
      </div>
      <div className="ask-scroll" ref={scroller}>
        {off ? (
          <div className="ask-intro">
            <p>Ask is off, so nothing is sent. Turn it on in Settings to ask an AI about your backups.</p>
            <button className="btn" title="Open Settings" onClick={() => go({ name: "settings" })}>
              Open Settings
            </button>
          </div>
        ) : !assistant.available ? (
          <div className="ask-intro">
            <p>{assistant.reason}</p>
          </div>
        ) : messages.length === 0 ? (
          <div className="ask-intro">
            <p>
              Ask what takes the space in your backups, why a backup grew or failed, or how to do something in Keepr. Your question goes to{" "}
              {toolName} with Keepr's figures: your plans' settings, their folders and how big they are, and the recent runs. Files aren't
              named and nothing of their contents goes. The help's Ask page says exactly what goes.
            </p>
            <p>An answer can suggest a change to a plan. Nothing changes until you apply it, and you can undo it.</p>
            <div className="ask-suggest">
              {SUGGESTIONS.map((s) => (
                <button key={s} type="button" title="Ask this" onClick={() => void send(s)}>
                  <span className="grow">{s}</span>
                  <Icon name="forward" size={13} stroke={2} />
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((m, i) => {
            if (m.role === "user")
              return (
                <div key={i} className="ask-q">
                  {m.about && <span className="ask-about">{m.about}</span>}
                  {m.text}
                </div>
              );
            if (!m.text)
              return (
                <div key={i} className="ask-a working" role="status">
                  {busy && i === messages.length - 1 ? (
                    <>
                      {(reading ?? doing ?? "Thinking").replace(/…$/, "")}
                      {since > 0 && <span className="muted"> · {Math.round((Date.now() - since) / 1000)} s</span>}
                    </>
                  ) : (
                    "Thinking"
                  )}
                  <span className="dots" aria-hidden="true">
                    <i>.</i>
                    <i>.</i>
                    <i>.</i>
                  </span>
                </div>
              );
            return <Answer key={i} m={m} i={i} copied={copied} copy={(i, text) => void copy(i, text)} />;
          })
        )}
      </div>
      {detail && (
        <div className="ask-detail">
          <span className="ell" title={detail.text}>
            With the details of <b>{detail.label}</b>
          </span>
          <button type="button" className="link small" onClick={() => setDetail(null)} title="Ask without these details">
            Remove
          </button>
        </div>
      )}
      <form
        className="ask-input"
        onSubmit={(e) => {
          e.preventDefault();
          void send(q);
        }}
      >
        <textarea
          ref={box}
          rows={2}
          value={q}
          disabled={off || !assistant.available}
          placeholder={messages.length ? "Ask a follow-up…" : "Ask about your backups…"}
          aria-label="Your question"
          onChange={(e) => setQ(e.target.value)}
          onCompositionEnd={(e) => {
            composedAt.current = e.timeStamp;
          }}
          onKeyDown={(e) => {
            const composing = e.nativeEvent.isComposing || e.keyCode === 229 || e.timeStamp - composedAt.current < 100;
            if (e.key === "Enter" && !e.shiftKey && !composing) {
              e.preventDefault();
              void send(q);
            } else if (e.key === "Escape" && composing) e.stopPropagation();
          }}
        />
        {busy ? (
          <button type="button" className="btn stop" onClick={stop} title="Stop the answer">
            Stop
          </button>
        ) : (
          <button type="submit" className="btn" disabled={!q.trim() || off || !assistant.available} title="Send (Enter)">
            Send
          </button>
        )}
      </form>
    </aside>
  );
}
