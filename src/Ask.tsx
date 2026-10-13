// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// Ask: a chat about the backups, answered by an AI CLI already signed in on this Mac (as
// Tidemark's Ask is; src-tauri/src/assistant/). The first message carries what Keepr knows
// (src-tauri/src/ask.rs: the plans, their biggest folders and growth, the space stored, the recent
// runs), as the help's Ask page describes it; a follow-up resumes the CLI's session. An Ask
// button elsewhere (a size map's folder, a run in Activity) opens it with a question and its own
// details. A change an answer proposes is shown, checked and applied in AskChange.tsx.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { GROUP_NAME, loadModel, modelGroups, pickModel, providerOf, saveModel, useAssistant, type Provider } from "./assistant";
import { AskChange, splitChanges } from "./AskChange";
import { AskIcon } from "./AskButton";
import { useApp } from "./context";
import { closeHelp, useHelpOpen } from "./help/HelpDrawer";
import { mdToHtml } from "./md";

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

/** The toolbar's Ask button: green while the panel is open, as Help's is. */
export function AskToolbarButton() {
  const open = useAskState() !== 0;
  return (
    <button className={`btn askbtn${open ? " on" : ""}`} aria-pressed={open} onClick={toggleAsk} title="Ask an AI about your backups ⌘J">
      <AskIcon />
      Ask
    </button>
  );
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
  const composedAt = useRef(0);
  const scroller = useRef<HTMLDivElement>(null);
  const chatRef = useRef(chatId);
  chatRef.current = chatId;

  // Help opening closes Ask: they share the right edge.
  useEffect(() => {
    if (helpOpen && shown) closeAsk();
  }, [helpOpen]);

  useEffect(() => {
    const chunk = listen<{ chatId: string; text: string }>("ask-chunk", (e) => {
      if (e.payload.chatId !== chatRef.current) return;
      setMessages((m) => {
        const last = m[m.length - 1];
        if (!last || last.role !== "assistant") return m;
        return [...m.slice(0, -1), { ...last, text: last.text + e.payload.text }];
      });
    });
    const done = listen<{ chatId: string; sessionId: string | null; text: string; error: string | null }>("ask-done", (e) => {
      if (e.payload.chatId !== chatRef.current) return;
      const d = e.payload;
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

  const send = async (question: string) => {
    const text = question.trim();
    if (!text || busy || off) return;
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
    setMessages((m) => [...m, { role: "user", text, about: detail?.label }, { role: "assistant", text: "" }]);
    try {
      let prompt = `${about}Question: ${text}`;
      if (!resume) {
        const ctx = await api.askContext(plan);
        prompt = `${ctx}\n\n${prompt}`;
      }
      await api.ask(chatId, prompt, model, resume);
    } catch (e) {
      setMessages((m) => [...m.slice(0, -1), { role: "assistant", text: String(e), error: true }]);
      setBusy(false);
    }
  };

  const fresh = () => {
    if (busy) void api.askCancel(chatId);
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
            <div className="ask-chips">
              {SUGGESTIONS.map((s) => (
                <button key={s} type="button" className="chip" title="Ask this" onClick={() => void send(s)}>
                  {s}
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
                  Thinking
                  <span className="dots" aria-hidden="true">
                    <i>.</i>
                    <i>.</i>
                    <i>.</i>
                  </span>
                </div>
              );
            const { text, changes } = splitChanges(m.text);
            return (
              <div key={i} className="ask-turn">
                <div className={`ask-a${m.error ? " error" : ""}`} dangerouslySetInnerHTML={{ __html: mdToHtml(text) }} />
                {m.done && changes.map((c, j) => <AskChange key={j} raw={c.raw} bad={c.bad} />)}
                {!m.error && (
                  <button type="button" className="btn small ask-copy" title="Copy this answer" onClick={() => void copy(i, text)}>
                    {copied === i ? "Copied" : "Copy"}
                  </button>
                )}
              </div>
            );
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
          <button type="button" className="btn stop" onClick={() => void api.askCancel(chatId)} title="Stop the answer">
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
