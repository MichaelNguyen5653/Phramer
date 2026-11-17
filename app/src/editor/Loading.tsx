// The editor window while its first image and the drawing engine load. A
// window opened without a pre-warmed one (a pin's Load into editor, for
// one) shows at once and waits here, so the click is answered within a
// frame; the title bar works meanwhile, so it can be moved or closed.

import type { Backend } from "./backend";
import { Icon } from "./Icon";

export function LoadingEditor({ backend }: { backend: Backend }) {
  return (
    <div className="empty" data-testid="editor-loading">
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
        <p className="empty-hint loading-hint">Opening the image…</p>
      </div>
    </div>
  );
}
