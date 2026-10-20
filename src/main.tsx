// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "./styles.css";
import App from "./App";
import Tray from "./Tray";
import { applyTheme, savedTheme } from "./Settings";
import { isMac, setSystemDrive } from "./platform";
import { api } from "./api";

// WebKit's own right-click menu offers Reload, which throws away the app's state. Keep it only
// where it's useful — in text fields and on selected text — and leave custom menus to their owners.
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (t?.closest("input, textarea, [contenteditable]:not([contenteditable='false'])")) return;
  if (window.getSelection()?.toString()) return;
  e.preventDefault();
});

applyTheme(savedTheme());

// macOS's room for the traffic lights and its drag areas (html.mac in styles.css); Windows' look
// (html.windows: its own title bar, Segoe UI).
document.documentElement.classList.add(isMac ? "mac" : "windows");
// Paths are shown with the system drive on Windows (platform.ts shown).
if (!isMac) void api.systemDrive().then(setSystemDrive, () => {});

const tray = new URLSearchParams(location.search).get("view") === "tray";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{tray ? <Tray /> : <App />}</React.StrictMode>,
);
