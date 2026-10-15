// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The question ⌘Q and Quit Keepr ask first (ask_to_quit in src-tauri/src/lib.rs sends `quit-requested`): quitting
// stops the backups, where closing the window doesn't. Esc or a click outside is Cancel.

import { useEffect, useState } from "react";
import { api, on } from "./api";
import { useApp } from "./context";
import { Mark } from "./icons";
import { jobTitle, Sheet } from "./ui";
import { draftName, useDraft } from "./draft";

export function QuitDialog() {
  const { job } = useApp();
  const draft = useDraft();
  const [shown, setShown] = useState(false);
  useEffect(() => on<null>("quit-requested", () => setShown(true)), []);
  if (!shown) return null;
  const cancel = () => {
    setShown(false);
    void api.quitCancelled();
  };
  return (
    <Sheet
      title="Quit Keepr?"
      icon={<Mark size={44} />}
      width={460}
      onClose={cancel}
      foot={
        <>
          <button className="btn" onClick={cancel}>
            Cancel
          </button>
          <button className="btn primary" autoFocus onClick={() => void api.quitNow()}>
            Quit
          </button>
        </>
      }
    >
      <div className="sheet-body">
        <p style={{ margin: 0 }}>
          {job ? `Keepr is ${jobTitle(job).replace(/^./, (c) => c.toLowerCase())}, and quitting stops it. ` : ""}
          Scheduled backups won't run until Keepr is open again. To keep backing up from the menu bar, close the window instead.
        </p>
        {draft && (
          <p style={{ margin: "10px 0 0", color: "var(--amber)" }}>
            {draftName(draft) === "A new plan" ? "The new plan you started" : draftName(draft)} hasn't been created yet, and quitting
            discards it.
          </p>
        )}
      </div>
    </Sheet>
  );
}
