import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "./styles.css";
import App from "./App";
import Tray from "./Tray";
import { applyTheme, savedTheme } from "./Settings";

applyTheme(savedTheme());

const tray = new URLSearchParams(location.search).get("view") === "tray";
if (tray) document.documentElement.classList.add("tray");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{tray ? <Tray /> : <App />}</React.StrictMode>,
);
