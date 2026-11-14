// Stands in for the Tauri runtime in the browser tests: the editor's
// commands answer as the daemon would, and every call is recorded in
// `window.__calls` for the tests to read. Events the daemon sends are
// raised with `window.__emit`; `__addImage` gives the window another image
// (Open in Editor while it is open) and `__setShortcut` changes a key as
// the settings window would.

export interface MockCall {
  cmd: string;
  args: unknown;
  headers?: Record<string, string>;
}

export interface MockOptions {
  /** Opened with no image (the tray's Open Editor). */
  empty?: boolean;
}

declare global {
  interface Window {
    __calls: MockCall[];
    __emit(event: string, payload?: unknown): void;
    __addImage(variant: number): void;
    __setShortcut(name: string, value: string): void;
  }
}

/** Runs in the page before it loads (`page.addInitScript`). */
export function installTauriMock(options: MockOptions = {}): void {
  const width = 640;
  const height = 400;
  // A light "window" with a dark title bar, a blue panel and a checkered
  // block, so pixel-reading tools have something to show; other variants
  // move and recolour the block, so each image looks its own
  const makeImage = (variant: number) => {
    const image = new Uint8Array(8 + width * height * 4);
    const view = new DataView(image.buffer);
    view.setUint32(0, width, true);
    view.setUint32(4, height, true);
    const shift = variant * 120;
    const colors = [
      [30, 90, 200],
      [200, 60, 60],
      [40, 150, 80],
    ][variant % 3];
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        let pixel = [244 - Math.floor(x / 16), 246 - Math.floor(y / 20), 250];
        if (y < 40) pixel = [40, 44, 52];
        else if (x > 400 - shift && x < 600 - shift && y > 80 && y < 220)
          pixel = (Math.floor(x / 10) + Math.floor(y / 10)) % 2 ? colors : [250, 210, 40];
        else if (x > 40 && x < 360 && y > 260 && y < 280) pixel = [60, 60, 66];
        const at = 8 + (y * width + x) * 4;
        image.set([pixel[0], pixel[1], pixel[2], 255], at);
      }
    }
    return image;
  };
  const images: Uint8Array[] = options.empty ? [] : [makeImage(0)];
  const settings = {
    color: [229, 72, 77],
    palette: [null, [255, 255, 255], [0, 0, 0], [128, 0, 0], [255, 0, 0], [255, 255, 0], [0, 255, 0], [0, 128, 0], [0, 255, 255], [0, 0, 255], [255, 0, 255], [128, 0, 128]],
    sizes: { pencil: 3, line: 3, arrow: 3, shape: 3, selection: 3, rectangle: 3, circle: 3, marker: 5, text: 8, circle_count: 1, pixelate: 2, invert: 3, move_object: 3 },
    thickness: 3,
    arrow_head: "default",
    reverse_arrow: false,
    shape_kind: "square",
    shape_style: "outline",
    rectangle_style: "filled",
    highlight_outline: 2,
    redact: "frosted",
    undo_limit: 100,
    shortcuts: {
      TYPE_PENCIL: "P", TYPE_DRAWER: "D", TYPE_ARROW: "A", TYPE_SELECTION: "S", TYPE_RECTANGLE: "R", TYPE_CIRCLE: "C",
      TYPE_MARKER: "M", TYPE_TEXT: "T", TYPE_PIXELATE: "B", TYPE_INVERT: "I", TYPE_MOVE_OBJECT: "G", TYPE_UNDO: "Ctrl+Z",
      TYPE_REDO: "Ctrl+Shift+Z", TYPE_COPY: "Ctrl+C", TYPE_COPY_FILE: "Ctrl+Shift+C", TYPE_SAVE: "Ctrl+S", TYPE_EXIT: "Ctrl+Q",
      TYPE_OPEN_APP: "Ctrl+O", TYPE_OCR: "O", TYPE_DELETE_CURRENT_TOOL: "Del", TYPE_COMMIT_CURRENT_TOOL: "Ctrl+Return",
      TYPE_SHAPE: "", TYPE_CIRCLECOUNT: "", TYPE_PIN: "", TYPE_SIZEINCREASE: "", TYPE_SIZEDECREASE: "",
    } as Record<string, string>,
    font_family: "",
  };
  window.__calls = [];
  let callbacks = 0;
  const handlers = new Map<number, (event: unknown) => void>();
  const listeners: { event: string; handler: number }[] = [];
  window.__emit = (event: string, payload?: unknown) => {
    for (const listener of listeners.filter((entry) => entry.event === event)) {
      handlers.get(listener.handler)?.({ event, id: listener.handler, payload });
    }
  };
  window.__addImage = (variant: number) => {
    images.push(makeImage(variant));
    window.__emit("editor:image-added", images.length - 1);
  };
  window.__setShortcut = (name: string, value: string) => {
    settings.shortcuts[name] = value;
    window.__emit("settings-changed");
  };
  const internals = {
    metadata: { currentWindow: { label: "editor-1" }, currentWebview: { windowLabel: "editor-1", label: "editor-1" } },
    transformCallback: (callback: (event: unknown) => void) => {
      const id = ++callbacks;
      handlers.set(id, callback);
      return id;
    },
    unregisterCallback: (id: number) => handlers.delete(id),
    convertFileSrc: (path: string) => path,
    async invoke(cmd: string, args: unknown, options?: { headers?: Record<string, string> }) {
      window.__calls.push({
        cmd,
        args: args instanceof Uint8Array ? `${args.length} bytes` : args,
        ...(options?.headers ? { headers: options.headers } : {}),
      });
      switch (cmd) {
        case "editor_image": {
          const index = (args as { index?: number } | undefined)?.index ?? 0;
          const image = images[index];
          return image ? image.buffer.slice(0) : new ArrayBuffer(0);
        }
        case "editor_settings":
          return JSON.parse(JSON.stringify(settings));
        case "editor_empty_state":
          return { capture_hotkey: "Meta+Shift+x", print_screen: true, auto_open_in_editor: false, editor_key: "E" };
        case "editor_font_families":
          return ["Arial", "Cambria", "Segoe UI"];
        case "editor_font_faces":
          return new ArrayBuffer(4);
        case "editor_export":
          return options?.headers?.["x-action"] === "save_all"
            ? [{ text: `Capture saved as C:\\Pictures\\Phramer set\\${options.headers["x-number"]} - capture.png`, error: false }]
            : [{ text: "Capture saved to clipboard.", error: false }];
        case "editor_save_all_begin":
          return "C:\\Pictures\\Phramer set";
        case "editor_save_all_end":
          return [{ text: "3 images saved to C:\\Pictures\\Phramer set", error: false }];
        case "plugin:event|listen": {
          const { event, handler } = args as { event: string; handler: number };
          listeners.push({ event, handler });
          return handler;
        }
        case "plugin:event|unlisten": {
          const { eventId } = args as { eventId: number };
          const at = listeners.findIndex((entry) => entry.handler === eventId);
          if (at >= 0) listeners.splice(at, 1);
          return null;
        }
        default:
          return null;
      }
    },
  };
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals });
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", { value: { unregisterListener: () => {} } });
}
