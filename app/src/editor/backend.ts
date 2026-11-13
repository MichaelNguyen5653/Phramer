// What the editor asks of the daemon (app/src-tauri/src/editor.rs), and the
// window it lives in.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Rendered } from "./engine";
import type { EditorSettings } from "./state";

export type ExportAction = "copy" | "copy_file" | "save" | "save_as" | "pin" | "ocr" | "open_app";

export interface Notice {
  text: string;
  error: boolean;
}

/** A setting the editor saves; each is an existing key the overlay uses. */
export type SettingChange =
  | { key: "draw_color"; color: [number, number, number] }
  | { key: "tool_size"; tool: string; size: number }
  | { key: "shape"; kind: string; style: string }
  | { key: "arrow"; head: string; reversed: boolean }
  | { key: "rectangle_style"; style: string }
  | { key: "highlight_outline"; width: number }
  | { key: "redact"; style: string };

export interface Image {
  width: number;
  height: number;
  rgba: Uint8Array;
}

/** The window's image: 8 bytes of size, then RGBA. */
export function parseImage(buffer: ArrayBuffer): Image {
  const view = new DataView(buffer);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  return { width, height, rgba: new Uint8Array(buffer, 8, width * height * 4) };
}

export const backend = {
  async image(): Promise<Image> {
    return parseImage(await invoke<ArrayBuffer>("editor_image"));
  },
  settings(): Promise<EditorSettings> {
    return invoke<EditorSettings>("editor_settings");
  },
  set(change: SettingChange): Promise<void> {
    return invoke("editor_set", { change });
  },
  exportImage(action: ExportAction, image: Rendered): Promise<Notice[]> {
    const body = new Uint8Array(image.rgba.buffer, image.rgba.byteOffset, image.rgba.byteLength);
    return invoke<Notice[]>("editor_export", body, {
      headers: { "x-action": action, "x-width": String(image.width), "x-height": String(image.height) },
    });
  },
  /** The image is on screen; `timing` is how long the page took. */
  ready(timing?: { page_ms: number; image_ms: number }): Promise<void> {
    return invoke("editor_ready", { timing: timing ?? null });
  },
  minimize(): Promise<void> {
    return getCurrentWindow().minimize();
  },
  toggleMaximize(): Promise<void> {
    return getCurrentWindow().toggleMaximize();
  },
  /** Closes without asking again. */
  destroy(): Promise<void> {
    return getCurrentWindow().destroy();
  },
  /** Alt+F4 and the taskbar's Close: `handler` decides. */
  onCloseRequested(handler: () => boolean): Promise<() => void> {
    return getCurrentWindow().onCloseRequested((event) => {
      if (!handler()) event.preventDefault();
    });
  },
};

export type Backend = typeof backend;
