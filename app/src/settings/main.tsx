// The settings window's entry point: the settings load, the page draws,
// then the daemon is told to show the window.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "../editor/editor.css";
import "./settings.css";
import { backend, showWhenPainted } from "./backend";
import { SettingsApp } from "./Settings";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  try {
    const initial = await backend.snapshot();
    root.render(
      <React.StrictMode>
        <SettingsApp initial={initial} backend={backend} />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The settings could not be read: {String(error)}</p>);
  }
  showWhenPainted();
}

void start();
