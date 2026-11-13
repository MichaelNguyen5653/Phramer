// The editor window's entry point: the engine, the image and the settings
// load together, then the editor draws and tells the daemon to show the
// window.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "./editor.css";
import wasmUrl from "./phramer_annotate.wasm?url";
import { backend } from "./backend";
import { Editor } from "./Editor";
import { Engine } from "./engine";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  const asked = performance.now();
  try {
    const [engine, image, settings] = await Promise.all([
      Engine.load(wasmUrl),
      backend.image(),
      backend.settings(),
    ]);
    root.render(
      <React.StrictMode>
        <Editor
          engine={engine}
          image={image}
          settings={settings}
          backend={backend}
          onReady={() => void backend.ready({ page_ms: performance.now(), image_ms: performance.now() - asked })}
        />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The image could not be opened: {String(error)}</p>);
    void backend.ready();
  }
}

void start();
