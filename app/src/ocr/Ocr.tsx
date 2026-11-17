// The OCR results window: 15.0.0's `OcrResultsWindow`. The text of one
// capture, editable; its layout applied live (no new recognition); a
// language picker once more than one is installed; Try harder; Copy all.
// Texts are 15.0.0's.

import { useEffect, useRef, useState } from "react";
import { Copy, X } from "lucide-react";
import type { Backend, Effort, OcrLanguage, Recognition } from "./backend";

/** The busy indicator appears only when recognition is noticeably slow. */
export const BUSY_DELAY_MS = 200;
/** How long Copy all says it copied. */
const COPIED_MS = 1200;

const HINT_BEFORE =
  "To improve OCR result, capture image close up and make sure image is not blurry for best result";
const HINT_EXACT = "Exact text, read from the window itself.";
const HINT_RECOGNIZED = "Recognized from the captured image. Capture close up and unblurred for the best result.";
const LAYOUT_TIP =
  "Preserve keeps indentation and the gaps between columns, so code and terminal output paste back as they looked. Join merges lines that continue the same paragraph, which suits prose.";
const HARDER_TIP =
  "Recognize again with every strategy the tool has, including thresholding and a larger upscale. Slower.";
const LAYOUTS = ["Preserve", "Plain lines", "Join wrapped"] as const;

export function Ocr({ initial, backend }: { initial: { layout: number; language: string }; backend: Backend }) {
  const [layout, setLayout] = useState(Math.min(Math.max(initial.layout, 0), 2));
  const [language, setLanguage] = useState(initial.language);
  const [languages, setLanguages] = useState<OcrLanguage[]>([]);
  const [result, setResult] = useState<Recognition | null>(null);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const run = useRef(0);
  const copiedTimer = useRef<number | undefined>(undefined);

  // A newer run supersedes an older one; its answer is ignored
  const recognize = (tag: string, effort: Effort) => {
    const id = ++run.current;
    setResult(null);
    setText("");
    setError(null);
    const delay = window.setTimeout(() => {
      if (run.current === id) setBusy(true);
    }, BUSY_DELAY_MS);
    backend
      .recognize(tag, effort)
      .then((answer) => {
        if (run.current !== id) return;
        setResult(answer);
      })
      .catch((problem) => {
        if (run.current === id) setError(String(problem));
      })
      .finally(() => {
        window.clearTimeout(delay);
        if (run.current === id) setBusy(false);
      });
  };

  // Once, at start (StrictMode runs effects twice in development)
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    recognize(initial.language, "normal");
    backend
      .languages()
      .then((found) => {
        // With one language there is nothing to choose
        if (found.length < 2) return;
        setLanguages(found);
        // A language no longer installed falls back to the default
        if (initial.language && !found.some((entry) => entry.tag === initial.language)) setLanguage("");
      })
      .catch(() => {});
  }, []);

  // The text follows the result and the layout; editing it is the user's
  useEffect(() => {
    if (result?.ok) setText(result.texts[layout] ?? "");
  }, [result, layout]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void backend.close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [backend]);

  const chooseLayout = (value: number) => {
    setLayout(value);
    void backend.setLayout(value).catch((problem) => setError(String(problem)));
  };

  const chooseLanguage = (tag: string) => {
    setLanguage(tag);
    void backend.setLanguage(tag).catch((problem) => setError(String(problem)));
    recognize(tag, "normal");
  };

  const copyAll = () => {
    backend
      .copy(text)
      .then(() => {
        setCopied(true);
        window.clearTimeout(copiedTimer.current);
        copiedTimer.current = window.setTimeout(() => setCopied(false), COPIED_MS);
      })
      .catch((problem) => setError(String(problem)));
  };

  const hint = result === null ? HINT_BEFORE : result.exact ? HINT_EXACT : HINT_RECOGNIZED;
  const status = busy ? "Recognizing text…" : (error ?? (result && !result.ok ? result.message : null));

  return (
    <div className="ocr" data-testid="ocr">
      <header className="panel-titlebar" data-tauri-drag-region>
        <span className="panel-title" data-tauri-drag-region>
          Extracted text
        </span>
        <button type="button" className="panel-close" aria-label="Close" title="Close (Esc)" onClick={() => void backend.close()}>
          <X className="icon" aria-hidden="true" />
        </button>
      </header>

      <main className="ocr-body">
        <p className={`ocr-hint${result?.exact ? " exact" : ""}`} data-testid="ocr-hint" title={result?.diagnostics || undefined}>
          {hint}
        </p>

        <div className="ocr-options">
          {languages.length > 1 && (
            <label className="ocr-option" data-testid="ocr-language">
              Language:
              <select className="field-select" value={language} onChange={(event) => chooseLanguage(event.target.value)}>
                <option value="">System default</option>
                {languages.map((entry) => (
                  <option key={entry.tag} value={entry.tag}>
                    {entry.name || entry.tag}
                  </option>
                ))}
              </select>
            </label>
          )}
          <label className="ocr-option" title={LAYOUT_TIP} data-testid="ocr-layout">
            Layout:
            <select className="field-select" value={layout} onChange={(event) => chooseLayout(Number(event.target.value))}>
              {LAYOUTS.map((name, index) => (
                <option key={name} value={index}>
                  {name}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="ocr-status" aria-live="polite">
          {status && (
            <>
              {busy && <span className="spinner" aria-hidden="true" />}
              <p
                className={`message${!busy && (error || (result && !result.ok)) ? " error" : ""}`}
                role={busy ? "status" : "alert"}
                data-testid="ocr-status"
                title={result?.diagnostics || undefined}
              >
                {status}
              </p>
            </>
          )}
        </div>

        <textarea
          className="ocr-text"
          data-testid="ocr-text"
          aria-label="Extracted text"
          spellCheck={false}
          value={text}
          onChange={(event) => setText(event.target.value)}
        />
      </main>

      <footer className="ocr-actions">
        <button
          type="button"
          className="button"
          data-testid="ocr-try-harder"
          title={HARDER_TIP}
          // Nothing to improve until a recognised result exists; exact text
          // has no recognition error in it
          disabled={result === null || result.exact}
          onClick={() => recognize(language, "high")}
        >
          Try harder
        </button>
        <button type="button" className="button primary" data-testid="ocr-copy" disabled={busy || text === ""} onClick={copyAll}>
          <Copy className="icon" aria-hidden="true" />
          {copied ? "Copied" : "Copy all"}
        </button>
      </footer>
    </div>
  );
}
