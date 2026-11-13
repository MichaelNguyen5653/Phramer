// Stands in for the daemon in the settings and welcome browser tests. It
// starts from the snapshot the daemon really sends for default settings
// (fixtures/settings-default.json, checked by a Rust test) and answers
// each command as settings_ui.rs does, recording every call in
// `window.__calls`.

import type { MockCall } from "./tauri-mock";

export interface MockOptions {
  snapshot: unknown;
  label: string;
  mandatory?: boolean;
}

declare global {
  interface Window {
    __calls: MockCall[];
  }
}

/** Runs in the page before it loads (`page.addInitScript`). */
export function installSettingsMock(options: MockOptions): void {
  type Row = { name: string; description: string; value: string; default: string };
  type State = {
    values: Record<string, unknown>;
    defaults: Record<string, unknown>;
    shortcuts: Row[];
    snipping_claims_print_screen: boolean;
    hotkey_failures: string[];
  };
  const state = JSON.parse(JSON.stringify(options.snapshot)) as State;
  const copy = () => JSON.parse(JSON.stringify(state));
  const canonical = (text: string) => {
    const parts = text.trim().endsWith("++")
      ? [...text.trim().slice(0, -2).split("+"), "+"]
      : text.trim().split("+").filter(Boolean);
    const all = parts.map((part) => part.toLowerCase());
    return all.length ? [...all.slice(0, -1).sort(), all[all.length - 1]].join("+") : "";
  };
  // 2026-11-13 09:41:07, a Friday
  const TIME: Record<string, string> = {
    Y: "2026", y: "26", C: "20", m: "11", d: "13", e: "13", j: "317", H: "09", I: "09", M: "41", S: "07",
    F: "2026-11-13", V: "46", u: "5",
  };
  window.__calls = [];
  let callbacks = 0;
  const internals = {
    metadata: { currentWindow: { label: options.label }, currentWebview: { windowLabel: options.label, label: options.label } },
    transformCallback: () => ++callbacks,
    unregisterCallback: () => {},
    convertFileSrc: (path: string) => path,
    async invoke(cmd: string, args: Record<string, unknown>) {
      window.__calls.push({ cmd, args });
      switch (cmd) {
        case "settings_snapshot":
        case "settings_pause_hotkeys":
          return copy();
        case "settings_set": {
          const key = args.key as string;
          const value = args.value;
          if (key === "undoLimit" && ((value as number) < 0 || (value as number) > 999)) {
            throw `${value} is not allowed for undoLimit`;
          }
          state.values[key] = value;
          return copy();
        }
        case "settings_reset":
          state.values[args.key as string] = state.defaults[args.key as string];
          return copy();
        case "settings_reset_all":
          for (const key of Object.keys(state.defaults)) {
            if (key !== "startupLaunch") state.values[key] = state.defaults[key];
          }
          return copy();
        case "settings_set_shortcut": {
          const name = args.name as string;
          const sequence = args.sequence as string;
          const row = state.shortcuts.find((entry) => entry.name === name);
          if (!row) throw `${name} is not a shortcut`;
          const owner = sequence
            ? state.shortcuts.find((entry) => entry.name && entry.name !== name && canonical(entry.value) === canonical(sequence))
            : undefined;
          if (owner && owner.value !== owner.default) {
            throw `${owner.value} is already used by \u201c${owner.description}\u201d`;
          }
          if (owner) owner.value = "";
          row.value = sequence ? sequence.slice(0, -1) + sequence.slice(-1).toLowerCase() : "";
          return copy();
        }
        case "settings_print_screen":
          state.snipping_claims_print_screen = !args.phramer;
          return copy();
        case "settings_preview_pattern": {
          const pattern = args.pattern as string;
          return pattern
            .replace(/%([A-Za-z])/g, (all, c: string) => TIME[c] ?? all)
            .replace(/\//g, "\u2044")
            .replace(/:/g, "-");
        }
        case "settings_choose_folder":
          state.values.savePath = "D:\\Captures";
          return copy();
        case "settings_export":
          return "C:\\Users\\you\\Desktop\\phramer.ini";
        case "settings_import":
          return copy();
        case "welcome_state":
          return {
            capture_hotkey: state.shortcuts.find((row) => row.name === "TAKE_SCREENSHOT")?.value ?? "",
            print_screen: !state.snipping_claims_print_screen,
            startup_launch: state.values.startupLaunch,
            mandatory: Boolean(options.mandatory),
            version: "16.0.0",
            store_build: false,
            snipping_tool_installed: true,
          };
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
