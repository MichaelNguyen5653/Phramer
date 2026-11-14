// What the editor asks of the daemon (app/src-tauri/src/editor.rs), and the
// window it lives in.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { FontFace, Rendered } from "./engine";
import type { EditorSettings } from "./state";

export type ExportAction = "copy" | "copy_file" | "save" | "save_as" | "pin" | "ocr" | "open_app" | "save_all";

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
  | { key: "redact"; style: string }
  | { key: "font_family"; family: string };

export interface Image {
  width: number;
  height: number;
  rgba: Uint8Array;
}

/** What the empty editor says about getting an image into it (15.0.0's
 * `updateEmptyState`). */
export interface EmptyState {
  /** `TAKE_SCREENSHOT`, as stored. */
  capture_hotkey: string;
  /** Print Screen reaches Phramer rather than Windows' snipping tool. */
  print_screen: boolean;
  auto_open_in_editor: boolean;
  /** `TYPE_OPEN_IN_EDITOR`, as stored. */
  editor_key: string;
}

/** A family's faces as `editor_font_faces` sends them: a count, then per
 * face bold, italic, collection index and length (u32, little-endian) and
 * the font file. */
export function parseFaces(family: string, buffer: ArrayBuffer): FontFace[] {
  const view = new DataView(buffer);
  const faces: FontFace[] = [];
  if (buffer.byteLength < 4) return faces;
  const count = view.getUint32(0, true);
  let at = 4;
  for (let n = 0; n < count && at + 16 <= buffer.byteLength; n++) {
    const length = view.getUint32(at + 12, true);
    faces.push({
      family,
      bold: view.getUint32(at, true) !== 0,
      italic: view.getUint32(at + 4, true) !== 0,
      index: view.getUint32(at + 8, true),
      data: new Uint8Array(buffer, at + 16, length),
    });
    at += 16 + length;
  }
  return faces;
}

/** The window's image: 8 bytes of size, then RGBA. */
export function parseImage(buffer: ArrayBuffer): Image {
  const view = new DataView(buffer);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  return { width, height, rgba: new Uint8Array(buffer, 8, width * height * 4) };
}

export const backend = {
  /** The image at `index` among those the window was given; `null` when it
   * has not been given that many. A pre-warmed window waits for image 0; a
   * window opened empty has none. */
  async imageAt(index: number): Promise<Image | null> {
    const buffer = await invoke<ArrayBuffer>("editor_image", { index });
    return buffer.byteLength === 0 ? null : parseImage(buffer);
  },
  /** Another image was given to this window (Open in Editor while it is
   * open): `handler` fetches it with `imageAt`. */
  onImageAdded(handler: () => void): Promise<() => void> {
    return listen("editor:image-added", handler);
  },
  /** The settings file changed, here or elsewhere. */
  onSettingsChanged(handler: () => void): Promise<() => void> {
    return listen("settings-changed", handler);
  },
  settings(): Promise<EditorSettings> {
    return invoke<EditorSettings>("editor_settings");
  },
  /** For a window opened with no image (the tray's Open Editor). */
  emptyState(): Promise<EmptyState> {
    return invoke<EmptyState>("editor_empty_state");
  },
  /** The system's font families, sorted. */
  fontFamilies(): Promise<string[]> {
    return invoke<string[]>("editor_font_families");
  },
  /** Every installed face of `family`; none when it is not installed. */
  async fontFaces(family: string): Promise<FontFace[]> {
    return parseFaces(family, await invoke<ArrayBuffer>("editor_font_faces", { family }));
  },
  set(change: SettingChange): Promise<void> {
    return invoke("editor_set", { change });
  },
  /** `number`: the image's place in Save All's folder. */
  exportImage(action: ExportAction, image: Rendered, number?: number): Promise<Notice[]> {
    const body = new Uint8Array(image.rgba.buffer, image.rgba.byteOffset, image.rgba.byteLength);
    const headers: Record<string, string> = { "x-action": action, "x-width": String(image.width), "x-height": String(image.height) };
    if (number !== undefined) headers["x-number"] = String(number);
    return invoke<Notice[]>("editor_export", body, { headers });
  },
  /** Save All: asks for a folder and makes one for the images inside it;
   * `null` when cancelled. Each image then goes as `save_all`. */
  saveAllBegin(): Promise<string | null> {
    return invoke<string | null>("editor_save_all_begin");
  },
  /** Save All is over: what became of it. */
  saveAllEnd(): Promise<Notice[]> {
    return invoke<Notice[]>("editor_save_all_end");
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

/** Calls `apply` with the `[Shortcuts]` entries each time the settings
 * change (the settings window, `phramer config`, an edited file), so the
 * toolbar's keys follow at once. Returns the way to stop. */
export function followShortcuts(
  source: Pick<Backend, "settings" | "onSettingsChanged">,
  apply: (shortcuts: Record<string, string>) => void,
): Promise<() => void> {
  return source.onSettingsChanged(() => {
    source
      .settings()
      .then((settings) => apply(settings.shortcuts))
      .catch((error: unknown) => console.warn("cannot read the settings", error));
  });
}
