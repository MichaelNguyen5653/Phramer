// Stands in for the daemon in the OCR results window and App Launcher
// browser tests. Answers each command as ocr_ui.rs and launcher_ui.rs do,
// recording every call in `window.__calls`.

import type { MockCall } from "./tauri-mock";

export interface PanelMockOptions {
  label: string;
  /** ocr_init. */
  layout?: number;
  language?: string;
  /** ocr_languages. */
  languages?: { tag: string; name: string }[];
  /** ocr_recognize answers by effort; a missing one rejects. */
  normal?: unknown;
  high?: unknown;
  /** How long ocr_recognize and launcher_apps take, in ms. */
  delay?: number;
  /** launcher_init. */
  keepOpen?: boolean;
  /** launcher_apps. */
  apps?: { name: string; exe: string; graphics: boolean }[];
  /** Programs given an icon (a coloured tile) by launcher_icons. */
  icons?: Record<string, string>;
  /** launcher_launch rejects with this. */
  launchError?: string;
}

declare global {
  interface Window {
    __calls: MockCall[];
  }
}

/** Runs in the page before it loads (`page.addInitScript`). */
export function installPanelMock(options: PanelMockOptions): void {
  window.__calls = [];
  let callbacks = 0;
  const wait = () => new Promise((resolve) => setTimeout(resolve, options.delay ?? 0));
  const tile = (color: string) =>
    "data:image/svg+xml;base64," +
    btoa(
      `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect x="4" y="4" width="56" height="56" rx="14" fill="${color}"/><rect x="18" y="18" width="28" height="28" rx="6" fill="#fff" fill-opacity="0.85"/></svg>`,
    );
  const internals = {
    metadata: { currentWindow: { label: options.label }, currentWebview: { windowLabel: options.label, label: options.label } },
    transformCallback: () => ++callbacks,
    unregisterCallback: () => {},
    convertFileSrc: (path: string) => path,
    async invoke(cmd: string, args: Record<string, unknown>) {
      window.__calls.push({ cmd, args });
      switch (cmd) {
        case "ocr_init":
          return { layout: options.layout ?? 0, language: options.language ?? "" };
        case "ocr_languages":
          return options.languages ?? [];
        case "ocr_recognize": {
          await wait();
          const answer = args.effort === "high" ? options.high : options.normal;
          if (!answer) throw "Text recognition failed: no answer scripted";
          return JSON.parse(JSON.stringify(answer));
        }
        case "ocr_copy":
        case "ocr_set_layout":
        case "ocr_set_language":
          return null;
        case "launcher_init":
          return { keep_open: Boolean(options.keepOpen), file_name: "2026-11-13_09-41_ab12cd.png" };
        case "launcher_apps":
          await wait();
          return JSON.parse(JSON.stringify(options.apps ?? []));
        case "launcher_icons": {
          const found: Record<string, string> = {};
          for (const exe of args.exes as string[]) {
            const color = options.icons?.[exe];
            if (color) found[exe] = tile(color);
          }
          return found;
        }
        case "launcher_launch":
          if (options.launchError) throw options.launchError;
          return null;
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
