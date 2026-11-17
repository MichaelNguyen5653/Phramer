// Capture Launcher's entry point.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "../panel.css";
import "./capture-launcher.css";
import { backend } from "./backend";
import { CaptureLauncher } from "./CaptureLauncher";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  try {
    const initial = await backend.init();
    root.render(
      <React.StrictMode>
        <CaptureLauncher initial={initial} backend={backend} />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The capture launcher could not be shown: {String(error)}</p>);
  }
  // Shows the window once the first frame is on screen
  requestAnimationFrame(() => requestAnimationFrame(() => void backend.ready()));
}

void start();
