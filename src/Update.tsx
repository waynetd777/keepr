// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The update dialog: src-tauri/src/updates.rs sends what it shows as `update` events (the offer,
// the download's progress, or a message such as "You're up to date"), and its buttons answer.
// Esc or a click outside is Later on an offer and OK on a message; an install under way can't be
// put away.

import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import ReactMarkdown, { Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { api, on, type UpdateShown } from "./api";
import { bytes, longDate } from "./format";
import { Mark } from "./icons";
import { Sheet } from "./ui";

// Links in the release notes open in the browser, not in the window.
const NOTES: Components = {
  a: ({ href = "", children }) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        if (/^https?:/.test(href)) void openUrl(href);
      }}
    >
      {children}
    </a>
  ),
};

/** `scene` is screenshot mode's dialog, shown without asking Rust. */
export function UpdateDialog({ scene }: { scene?: UpdateShown | null }) {
  const [shown, setShown] = useState<UpdateShown | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    // One sent before this window loaded is still waiting for an answer.
    api.updateDialog().then((u) => u && setShown((cur) => cur ?? u));
    return on<UpdateShown | null>("update", (u) => {
      setShown(u);
      setBusy(false);
    });
  }, []);
  useEffect(() => {
    if (scene) setShown(scene);
  }, [scene]);

  if (!shown) return null;
  const later = () => {
    setShown(null);
    void api.updateLater();
  };
  const mark = <Mark size={44} />;

  if (shown.kind === "message") {
    return (
      <Sheet
        title={shown.title}
        icon={mark}
        width={460}
        onClose={later}
        foot={
          <button className="btn primary" autoFocus onClick={later}>
            OK
          </button>
        }
      >
        <div className="sheet-body">
          <p style={{ margin: 0, userSelect: "text" }}>{shown.text}</p>
        </div>
      </Sheet>
    );
  }

  if (shown.kind === "progress") {
    const { stage, downloaded, total } = shown;
    const p = stage !== "downloading" ? 100 : total ? Math.min(100, (downloaded / total) * 100) : 0;
    const title =
      stage === "restarting" ? "Restarting Keepr" : `${stage === "installing" ? "Installing" : "Downloading"} Keepr ${shown.version}`;
    const says =
      stage === "restarting"
        ? "Restarting…"
        : stage === "installing"
          ? "Installing…"
          : total
            ? `${bytes(downloaded)} of ${bytes(total)}`
            : downloaded
              ? bytes(downloaded)
              : "Starting the download…";
    return (
      <Sheet title={title} icon={mark} width={460} onClose={() => {}}>
        <div className="sheet-body">
          <div
            className="progress"
            style={{ height: 6 }}
            role="progressbar"
            aria-label="Update progress"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(p)}
          >
            <div className="solid" style={{ width: `${p}%`, transition: "width 0.3s" }} />
          </div>
          <span className="small muted" aria-live="polite">
            {says}
          </span>
        </div>
      </Sheet>
    );
  }

  const released = shown.date ? ` Released ${longDate(new Date(shown.date * 1000).toISOString())}.` : "";
  return (
    <Sheet
      title={`Keepr ${shown.version} is available`}
      subtitle={`You have ${shown.current}.${released}`}
      icon={mark}
      width={540}
      onClose={busy ? () => {} : later}
      foot={
        <>
          <button
            className="btn"
            disabled={busy}
            onClick={() => {
              setShown(null);
              void api.updateSkip();
            }}
          >
            Skip This Version
          </button>
          <span className="grow" />
          <button className="btn" disabled={busy} onClick={later}>
            Later
          </button>
          <button
            className="btn primary"
            disabled={busy}
            autoFocus
            onClick={() => {
              // Stays up until the download's progress replaces it.
              setBusy(true);
              void api.updateInstall();
            }}
          >
            Install and Restart
          </button>
        </>
      }
    >
      {shown.notes && (
        <div className="sheet-body">
          <span className="caps">What's new</span>
          <div className="help-text">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={NOTES}>
              {shown.notes}
            </ReactMarkdown>
          </div>
        </div>
      )}
    </Sheet>
  );
}
