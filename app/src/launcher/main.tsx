// App Launcher's entry point.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "../panel.css";
import "./launcher.css";
import { backend } from "./backend";
import { Launcher } from "./Launcher";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  try {
    const initial = await backend.init();
    root.render(
      <React.StrictMode>
        <Launcher initial={initial} backend={backend} />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The app list could not be shown: {String(error)}</p>);
  }
  // Shows the window once the first frame is on screen
  requestAnimationFrame(() => requestAnimationFrame(() => void backend.ready()));
}

void start();
