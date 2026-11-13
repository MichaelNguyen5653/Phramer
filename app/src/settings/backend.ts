// What the settings and welcome windows ask of the daemon
// (app/src-tauri/src/settings_ui.rs and welcome.rs), and their window.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Snapshot, Value } from "./types";

export interface WelcomeState {
  capture_hotkey: string;
  /** Phramer, not Windows' snipping tool, answers Print Screen. */
  print_screen: boolean;
  startup_launch: boolean;
  mandatory: boolean;
  version: string;
  store_build: boolean;
  snipping_tool_installed: boolean;
}

export const backend = {
  snapshot(): Promise<Snapshot> {
    return invoke<Snapshot>("settings_snapshot");
  },
  /** Saves one setting; the daemon applies it at once. */
  set(key: string, value: Value): Promise<Snapshot> {
    return invoke<Snapshot>("settings_set", { key, value });
  },
  reset(key: string): Promise<Snapshot> {
    return invoke<Snapshot>("settings_reset", { key });
  },
  resetAll(): Promise<Snapshot> {
    return invoke<Snapshot>("settings_reset_all");
  },
  setShortcut(name: string, sequence: string): Promise<Snapshot> {
    return invoke<Snapshot>("settings_set_shortcut", { name, sequence });
  },
  /** Lets go of the hotkeys while the recorder listens. */
  pauseHotkeys(paused: boolean): Promise<Snapshot> {
    return invoke<Snapshot>("settings_pause_hotkeys", { paused });
  },
  printScreen(phramer: boolean): Promise<Snapshot> {
    return invoke<Snapshot>("settings_print_screen", { phramer });
  },
  previewPattern(pattern: string): Promise<string> {
    return invoke<string>("settings_preview_pattern", { pattern });
  },
  /** `null` when the dialog is cancelled. */
  chooseFolder(): Promise<Snapshot | null> {
    return invoke<Snapshot | null>("settings_choose_folder");
  },
  exportFile(): Promise<string | null> {
    return invoke<string | null>("settings_export");
  },
  importFile(): Promise<Snapshot | null> {
    return invoke<Snapshot | null>("settings_import");
  },
  openSettings(): Promise<void> {
    return invoke("settings_open");
  },
  welcomeState(): Promise<WelcomeState> {
    return invoke<WelcomeState>("welcome_state");
  },
  welcomeReachedEnd(): Promise<void> {
    return invoke("welcome_reached_end");
  },
  /** The settings file changed, here or elsewhere. */
  onChanged(handler: () => void): Promise<() => void> {
    return listen("settings-changed", handler);
  },
  /** The page has painted: the window shows. */
  ready(): Promise<void> {
    return invoke("ui_ready");
  },
  minimize(): Promise<void> {
    return getCurrentWindow().minimize();
  },
  toggleMaximize(): Promise<void> {
    return getCurrentWindow().toggleMaximize();
  },
  close(): Promise<void> {
    return getCurrentWindow().close();
  },
  /** Alt+F4 and the taskbar's Close: `handler` decides. */
  onCloseRequested(handler: () => boolean): Promise<() => void> {
    return getCurrentWindow().onCloseRequested((event) => {
      if (!handler()) event.preventDefault();
    });
  },
};

export type Backend = typeof backend;

/** Shows the window once the first frame is on screen. */
export function showWhenPainted(): void {
  requestAnimationFrame(() => requestAnimationFrame(() => void backend.ready()));
}
