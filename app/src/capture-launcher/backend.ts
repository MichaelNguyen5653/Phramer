// What Capture Launcher asks of the daemon (app/src-tauri/src/capture_launcher_ui.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CaptureLauncherInit {
  monitors: { name: string; width: number; height: number }[];
  /** The monitor under the cursor. */
  current: number;
  /** The last capture's region; null before the first one. */
  last_region: Area | null;
}

export type Mode = "region" | "full";

export interface Choice {
  mode: Mode;
  monitor: number;
  region: Area;
  delay_s: number;
}

export const backend = {
  init(): Promise<CaptureLauncherInit> {
    return invoke<CaptureLauncherInit>("capture_launcher_init");
  },
  /** Starts the capture; without a delay the window goes at once. */
  take(choice: Choice): Promise<void> {
    return invoke("capture_launcher_take", { choice });
  },
  /** The capture was aborted or failed: the window is back. */
  onBack(handler: () => void): Promise<() => void> {
    return listen("capture-launcher:back", handler);
  },
  hide(): Promise<void> {
    return getCurrentWindow().hide();
  },
  /** The page has painted: the window shows. */
  ready(): Promise<void> {
    return invoke("ui_ready");
  },
  close(): Promise<void> {
    return getCurrentWindow().close();
  },
};

export type Backend = typeof backend;
