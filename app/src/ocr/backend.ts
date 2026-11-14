// What the OCR results window asks of the daemon (app/src-tauri/src/ocr_ui.rs).

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface OcrInit {
  /** `ocrTextLayout`: 0 Preserve, 1 Plain, 2 Join. */
  layout: number;
  /** `ocrLanguage`; empty for the system default. */
  language: string;
}

export interface OcrLanguage {
  tag: string;
  name: string;
}

export interface Recognition {
  ok: boolean;
  /** What the user is told when no text came back. */
  message: string | null;
  /** Read from the window itself rather than recognised. */
  exact: boolean;
  diagnostics: string;
  /** Preserve, Plain and Join. */
  texts: [string, string, string];
}

export type Effort = "normal" | "high";

export const backend = {
  init(): Promise<OcrInit> {
    return invoke<OcrInit>("ocr_init");
  },
  languages(): Promise<OcrLanguage[]> {
    return invoke<OcrLanguage[]>("ocr_languages");
  },
  recognize(language: string, effort: Effort): Promise<Recognition> {
    return invoke<Recognition>("ocr_recognize", { language, effort });
  },
  setLayout(layout: number): Promise<void> {
    return invoke("ocr_set_layout", { layout });
  },
  setLanguage(language: string): Promise<void> {
    return invoke("ocr_set_language", { language });
  },
  copy(text: string): Promise<void> {
    return invoke("ocr_copy", { text });
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
