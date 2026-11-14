// The OCR results window's entry point.

import React from "react";
import ReactDOM from "react-dom/client";
import "../tokens.css";
import "../panel.css";
import "./ocr.css";
import { backend } from "./backend";
import { Ocr } from "./Ocr";

async function start() {
  const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
  try {
    const initial = await backend.init();
    root.render(
      <React.StrictMode>
        <Ocr initial={initial} backend={backend} />
      </React.StrictMode>,
    );
  } catch (error) {
    root.render(<p className="load-error">The extracted text could not be shown: {String(error)}</p>);
  }
  // Shows the window once the first frame is on screen
  requestAnimationFrame(() => requestAnimationFrame(() => void backend.ready()));
}

void start();
