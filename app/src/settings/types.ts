// What the daemon sends the settings window (app/src-tauri/src/settings_ui.rs).

/** A setting's value as the page sees it. */
export type Value = boolean | number | string | (string | null)[] | number[];

export interface ShortcutRow {
  /** The `[Shortcuts]` entry; empty for a key that cannot be changed. */
  name: string;
  description: string;
  group: string;
  value: string;
  default: string;
  /** A system-wide hotkey rather than a key of the capture screen. */
  global: boolean;
}

export interface ToolbarChoice {
  id: number;
  name: string;
  description: string;
  icon: string;
}

export interface Snapshot {
  values: Record<string, Value>;
  defaults: Record<string, Value>;
  shortcuts: ShortcutRow[];
  toolbar: ToolbarChoice[];
  extensions: string[];
  /** Windows opens its snipping tool on Print Screen. */
  snipping_claims_print_screen: boolean;
  snipping_tool_installed: boolean;
  /** Key sequences wanted as hotkeys that are not registered. */
  hotkey_failures: string[];
  file: string;
  version: string;
  store_build: boolean;
}
