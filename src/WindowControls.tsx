// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The main window's own minimise, maximise and close buttons on Windows, where it has no title bar
// (src-tauri/tauri.windows.conf.json): at the top right, where Windows puts its own, as wide as
// they are and as tall as the toolbar, over every sheet as the native ones are. Close goes through
// the window's close request, as the native X did, so src-tauri/src/lib.rs hides the window and
// Keepr keeps backing up from the notification area. Nothing renders on macOS, where the window
// keeps its traffic lights. As Brainstead's are.
//
// Windows 11 offers its snap layouts over maximise only to a window that answers the system's
// hit-test there, which the page can't: src-tauri/src/snap_win.rs puts a window of its own over the
// button, placed from here, which does the click and says when the pointer is on it (the page gets
// no hover of its own there).

import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, on } from "./api";
import { isWindows } from "./platform";

const Glyph = ({ d }: { d: string }) => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" shapeRendering="crispEdges">
    <path d={d} fill="none" stroke="currentColor" strokeWidth="1" />
  </svg>
);

export function WindowControls() {
  const [maximised, setMaximised] = useState(false);
  const [hover, setHover] = useState(false);
  const max = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!isWindows) return;
    const w = getCurrentWindow();
    // The snap-layout window goes over the button, in the window's physical pixels.
    const place = () => {
      const b = max.current?.getBoundingClientRect();
      const r = window.devicePixelRatio || 1;
      api
        .windowMaxButton(b ? [Math.round(b.left * r), Math.round(b.top * r), Math.round(b.width * r), Math.round(b.height * r)] : null)
        .catch(() => {});
    };
    const read = () => {
      w.isMaximized().then(setMaximised, () => {});
      place();
    };
    read();
    window.addEventListener("resize", read);
    const offs = [w.onResized(read), w.onScaleChanged(read)].map((o) => o.catch(() => () => {}));
    const offHover = on<boolean>("window-controls-hover", setHover);
    return () => {
      window.removeEventListener("resize", read);
      offs.forEach((o) => void o.then((f) => f()));
      offHover();
      api.windowMaxButton(null).catch(() => {});
    };
  }, []);
  if (!isWindows) return null;
  const w = () => getCurrentWindow();
  return (
    <div className="winctl" role="group" aria-label="Window">
      <button type="button" tabIndex={-1} title="Minimise" aria-label="Minimise" onClick={() => void w().minimize()}>
        <Glyph d="M0 5.5h10" />
      </button>
      <button
        ref={max}
        type="button"
        tabIndex={-1}
        className={hover ? "hover" : undefined}
        title={maximised ? "Restore" : "Maximise"}
        aria-label={maximised ? "Restore" : "Maximise"}
        onClick={() => void w().toggleMaximize()}
      >
        {maximised ? <Glyph d="M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" /> : <Glyph d="M0.5 0.5h9v9h-9z" />}
      </button>
      <button
        type="button"
        tabIndex={-1}
        className="close"
        title="Close: Keepr keeps backing up from the notification area"
        aria-label="Close"
        onClick={() => void w().close()}
      >
        <Glyph d="M0 0l10 10M10 0L0 10" />
      </button>
    </div>
  );
}
