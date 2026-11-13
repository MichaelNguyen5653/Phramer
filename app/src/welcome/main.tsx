// The welcome window's entry point.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "../editor/editor.css";
import "../settings/settings.css";
import "./welcome.css";
import { backend, showWhenPainted } from "../settings/backend";
import { Welcome } from "./Welcome";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  try {
    const initial = await backend.welcomeState();
    root.render(
      <React.StrictMode>
        <Welcome initial={initial} backend={backend} />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The welcome could not be shown: {String(error)}</p>);
  }
  showWhenPainted();
}

void start();
