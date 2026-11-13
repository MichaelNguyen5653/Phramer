// The editor window's entry point: the engine and the image (then the
// settings) load together, then the editor draws and tells the daemon to
// show the window.

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
  let arrived = performance.now();
  try {
    const [engine, [image, settings]] = await Promise.all([
      Engine.load(wasmUrl),
      // A window built ahead during a capture waits here for its image; the
      // settings are read once it comes, with what the overlay saved meanwhile
      backend.image().then(async (image) => {
        arrived = performance.now();
        return [image, await backend.settings()] as const;
      }),
    ]);
    root.render(
      <React.StrictMode>
        <Editor
          engine={engine}
          image={image}
          settings={settings}
          backend={backend}
          onReady={() => void backend.ready({ page_ms: performance.now(), image_ms: performance.now() - arrived })}
        />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The image could not be opened: {String(error)}</p>);
    void backend.ready();
  }
}

void start();
