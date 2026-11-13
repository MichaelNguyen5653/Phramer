// Stands in for the Tauri runtime in the browser tests: the editor's
// commands answer as the daemon would, and every call is recorded in
// `window.__calls` for the tests to read.

export interface MockCall {
  cmd: string;
  args: unknown;
}

declare global {
  interface Window {
    __calls: MockCall[];
  }
}

/** Runs in the page before it loads (`page.addInitScript`). */
export function installTauriMock(): void {
  const width = 640;
  const height = 400;
  // A light "window" with a dark title bar, a blue panel and a checkered
  // block, so pixel-reading tools have something to show
  const image = new Uint8Array(8 + width * height * 4);
  const view = new DataView(image.buffer);
  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      let pixel = [244 - Math.floor(x / 16), 246 - Math.floor(y / 20), 250];
      if (y < 40) pixel = [40, 44, 52];
      else if (x > 400 && x < 600 && y > 80 && y < 220) pixel = (Math.floor(x / 10) + Math.floor(y / 10)) % 2 ? [30, 90, 200] : [250, 210, 40];
      else if (x > 40 && x < 360 && y > 260 && y < 280) pixel = [60, 60, 66];
      const at = 8 + (y * width + x) * 4;
      image.set([pixel[0], pixel[1], pixel[2], 255], at);
    }
  }
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
    },
  };
  window.__calls = [];
  let callbacks = 0;
  const internals = {
    metadata: { currentWindow: { label: "editor-1" }, currentWebview: { windowLabel: "editor-1", label: "editor-1" } },
    transformCallback: () => ++callbacks,
    unregisterCallback: () => {},
    convertFileSrc: (path: string) => path,
    async invoke(cmd: string, args: unknown) {
      window.__calls.push({ cmd, args: args instanceof Uint8Array ? `${args.length} bytes` : args });
      switch (cmd) {
        case "editor_image":
          return image.buffer.slice(0);
        case "editor_settings":
          return JSON.parse(JSON.stringify(settings));
        case "editor_export":
          return [{ text: "Capture saved to clipboard.", error: false }];
        case "plugin:event|listen":
          return ++callbacks;
        default:
          return null;
      }
    },
  };
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals });
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", { value: { unregisterListener: () => {} } });
}
