// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// A small Ask button beside something worth asking about: a size map's folder, a run in
// Activity. It opens Ask with a question and the thing's details; disabled, with the reason, when
// Ask is off in Settings, no AI tool is installed, or the caller says why not (`off`: the plan is
// left out of Ask, say).

import { useEffect, useState } from "react";
import { useAssistant } from "./assistant";
import { useApp } from "./context";

export function AskIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2.5 3.5A1.5 1.5 0 0 1 4 2h8a1.5 1.5 0 0 1 1.5 1.5v6A1.5 1.5 0 0 1 12 11H7l-3.2 2.7V11H4a1.5 1.5 0 0 1-1.5-1.5z" />
    </svg>
  );
}

export function AskButton({ onAsk, title, off }: { onAsk: () => void | Promise<void>; title: string; off?: string }) {
  const assistant = useAssistant();
  const { cfg } = useApp();
  const why = off ?? (cfg?.settings.ask === false ? "Ask is off in Settings" : undefined);
  // Opening Ask can wait on the caller (reading the thing's details), so the button waits too.
  const [busy, setBusy] = useState(false);
  // Why Ask couldn't open (the details unreadable, say), for a few seconds.
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    if (!failed) return;
    const id = window.setTimeout(() => setFailed(null), 6000);
    return () => window.clearTimeout(id);
  }, [failed]);
  return (
    <span className="tipwrap" title={why ?? (assistant.available ? undefined : (assistant.reason ?? undefined))}>
      <button
        className="btn small askbtn"
        disabled={!assistant.available || busy || !!why}
        title={why ?? title}
        onClick={() => {
          setBusy(true);
          setFailed(null);
          // Through then(), so a throw as well as a rejection clears busy.
          void Promise.resolve()
            .then(onAsk)
            .catch((e: unknown) => setFailed(`Couldn't open Ask: ${String(e)}`))
            .finally(() => setBusy(false));
        }}
      >
        <AskIcon />
        Ask
      </button>
      {failed && (
        <span className="small" role="alert" style={{ color: "var(--red)", marginLeft: 6 }}>
          {failed}
        </span>
      )}
    </span>
  );
}
