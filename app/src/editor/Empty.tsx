// The editor opened with no image (the tray's Open Editor, 15.0.0's
// `openEmpty`): how to capture one into it. The first image that comes
// replaces it with the editor.

import { useEffect, useRef, useState } from "react";
import type { Backend, EmptyState, Image } from "./backend";
import { Icon } from "./Icon";
import { parseShortcut, shortcutLabel } from "./tools";

/** A stored key as the editor writes it; the stored text when it is not
 * one the editor reads. */
function keyText(stored: string): string {
  const shortcut = parseShortcut(stored);
  return shortcut ? shortcutLabel(shortcut) : stored.trim();
}

/** 15.0.0's hint under the title (`updateEmptyState`). */
export function emptyHint(state: EmptyState): string {
  const keys: string[] = [];
  const capture = keyText(state.capture_hotkey);
  const isPrint = /^print(\s*screen)?$/i.test(capture);
  if (capture && !isPrint) keys.push(capture);
  // Only when Print Screen reaches Phramer, not Windows' snipping tool
  if (state.print_screen || isPrint) keys.push("Print Screen");
  const first = keys.length === 0 ? "Take a capture from the tray icon" : `Press ${keys.join(" or ")} to capture`;
  const editorKey = keyText(state.editor_key);
  const second = state.auto_open_in_editor
    ? "It opens here automatically."
    : editorKey
      ? `Then pick Editor (${editorKey}) on the capture toolbar.`
      : "Then pick Editor on the capture toolbar.";
  return `${first}. ${second}`;
}

export interface EmptyEditorProps {
  backend: Backend;
  /** The first image has come. */
  onImage(image: Image): void;
  /** Drawn: the window may show. */
  onReady(): void;
}

export function EmptyEditor({ backend, onImage, onReady }: EmptyEditorProps) {
  const [state, setState] = useState<EmptyState | null>(null);
  const shown = useRef(false);

  useEffect(() => {
    const read = () =>
      backend
        .emptyState()
        .then(setState)
        .catch(() => setState({ capture_hotkey: "", print_screen: false, auto_open_in_editor: false, editor_key: "" }))
        .finally(() => {
          if (!shown.current) {
            shown.current = true;
            requestAnimationFrame(onReady);
          }
        });
    void read();
    const settings = backend.onSettingsChanged(() => void read());
    // Open in Editor while it is open: the image is this window's first
    let fetching = false;
    const added = backend.onImageAdded(() => {
      if (fetching) return;
      fetching = true;
      backend
        .imageAt(0)
        .then((image) => (image ? onImage(image) : undefined))
        .finally(() => {
          fetching = false;
        });
    });
    return () => {
      void settings.then((stop) => stop());
      void added.then((stop) => stop());
    };
  }, [backend, onImage, onReady]);

  return (
    <div className="empty" data-testid="empty-editor">
      <header className="titlebar" data-tauri-drag-region>
        <span className="title" data-tauri-drag-region>
          Phramer Editor
        </span>
        <div className="window-buttons">
          <button type="button" aria-label="Minimize" title="Minimize" onClick={() => void backend.minimize()}>
            <Icon name="minus" />
          </button>
          <button type="button" aria-label="Maximize" title="Maximize" onClick={() => void backend.toggleMaximize()}>
            <Icon name="square" />
          </button>
          <button type="button" className="close" aria-label="Close" title="Close" onClick={() => void backend.destroy()}>
            <Icon name="x" />
          </button>
        </div>
      </header>
      <div className="empty-body">
        <span className="empty-icon">
          <Icon name="image-plus" />
        </span>
        <h1 className="empty-title">Perform a capture to load an image into the editor</h1>
        <p className="empty-hint" data-testid="empty-hint">
          {state ? emptyHint(state) : ""}
        </p>
      </div>
    </div>
  );
}
