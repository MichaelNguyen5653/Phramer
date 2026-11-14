// What App Launcher asks of the daemon (app/src-tauri/src/launcher_ui.rs).

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppEntry } from "./filter";

export interface LauncherInit {
  /** `keepOpenAppLauncher`. */
  keep_open: boolean;
  /** The capture's file name. */
  file_name: string;
}

export const backend = {
  init(): Promise<LauncherInit> {
    return invoke<LauncherInit>("launcher_init");
  },
  /** The Start Menu's apps; cached by the daemon after the first time. */
  apps(): Promise<AppEntry[]> {
    return invoke<AppEntry[]>("launcher_apps");
  },
  /** PNG data URLs by program; programs without an icon are left out. */
  icons(exes: string[]): Promise<Record<string, string>> {
    return invoke<Record<string, string>>("launcher_icons", { exes });
  },
  launch(exe: string): Promise<void> {
    return invoke("launcher_launch", { exe });
  },
  /** Windows' "Open with" dialog for the capture. */
  openWith(): Promise<void> {
    return invoke("launcher_open_with");
  },
  setKeepOpen(keepOpen: boolean): Promise<void> {
    return invoke("launcher_set_keep_open", { keepOpen });
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
