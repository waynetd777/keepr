// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// When the window opens: a welcome offering the help's Getting started guides, until "Don't ask me
// again" is ticked or Settings turns it off (settings.offerGuide). Once a launch, and not in
// screenshot mode.

import { useEffect, useState } from "react";
import { api } from "../api";
import { useApp } from "../context";
import { Mark } from "../icons";
import { Sheet } from "../ui";
import { openHelp } from "./HelpDrawer";

let offered = false;

export function GuideOffer() {
  const { cfg, refresh } = useApp();
  const [open, setOpen] = useState(false);
  const [never, setNever] = useState(false);
  const wanted = !!cfg && cfg.settings.offerGuide !== false;
  useEffect(() => {
    if (!wanted || offered) return;
    offered = true;
    let live = true;
    // A moment after the window has drawn, so it isn't lost under the first screen's loading.
    const t = window.setTimeout(() => {
      void api.scene().then((sc) => live && !sc && setOpen(true));
    }, 800);
    return () => {
      live = false;
      window.clearTimeout(t);
    };
  }, [wanted]);
  if (!open || !cfg) return null;
  const close = async (start: boolean) => {
    setOpen(false);
    if (start) openHelp({ guide: "guide-start", step: 0 });
    if (never) {
      await api.saveSettings({ ...cfg.settings, offerGuide: false });
      await refresh();
    }
  };
  return (
    <Sheet
      title="Welcome to Keepr"
      icon={<Mark size={36} />}
      width={500}
      onClose={() => void close(false)}
      foot={
        <>
          <label className="check" style={{ marginRight: "auto" }} title="Don't offer Getting started when Keepr opens">
            <input type="checkbox" checked={never} onChange={(e) => setNever(e.target.checked)} />
            Don't ask me again
          </label>
          <button className="btn" title="Close this and carry on" onClick={() => void close(false)}>
            Not Now
          </button>
          <button className="btn primary" autoFocus title="Open the first guide in the help" onClick={() => void close(true)}>
            Open Getting Started
          </button>
        </>
      }
    >
      <div className="sheet-body col" style={{ gap: 10 }}>
        <p style={{ margin: 0 }}>
          Getting started walks you through Keepr a step at a time: somewhere to keep backups, a plan, getting a file back, and more.
        </p>
        <p className="small muted" style={{ margin: 0 }}>
          To come back to it, open the help (press ? or click ? at the top right) and click Getting started.
        </p>
      </div>
    </Sheet>
  );
}
