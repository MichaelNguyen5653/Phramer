// The editor window's entry point: the engine and the first image (then the
// settings) load together, then the editor draws and tells the daemon to
// show the window. A window opened with no image shows how to get one.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "./editor.css";
import wasmUrl from "./phramer_annotate.wasm?url";
import { backend, type Image } from "./backend";
import { Editor } from "./Editor";
import { EmptyEditor } from "./Empty";
import { Engine } from "./engine";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  const engine = Engine.load(wasmUrl);
  const fail = (error: unknown) => {
    root.render(<p className="load-error">The image could not be opened: {String(error)}</p>);
    void backend.ready();
  };
  const show = async (first: Promise<Image>) => {
    let arrived = performance.now();
    const [loaded, [image, settings]] = await Promise.all([
      engine,
      // A window built ahead during a capture waits here for its image; the
      // settings are read once it comes, with what the overlay saved meanwhile
      first.then(async (image) => {
        arrived = performance.now();
        return [image, await backend.settings()] as const;
      }),
    ]);
    root.render(
      <React.StrictMode>
        <Editor
          engine={loaded}
          image={image}
          settings={settings}
          backend={backend}
          onReady={() => void backend.ready({ page_ms: performance.now(), image_ms: performance.now() - arrived })}
        />
      </React.StrictMode>,
    );
  };
  try {
    const first = backend.imageAt(0);
    if ((await first) !== null) {
      await show(first as Promise<Image>);
      return;
    }
    root.render(
      <React.StrictMode>
        <EmptyEditor
          backend={backend}
          onImage={(image) => void show(Promise.resolve(image)).catch(fail)}
          onReady={() => void backend.ready()}
        />
      </React.StrictMode>,
    );
  } catch (error) {
    fail(error);
  }
}

void start();
