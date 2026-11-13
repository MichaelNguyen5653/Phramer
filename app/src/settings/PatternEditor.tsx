// The file name pattern with 15.0.0's helper buttons and a live preview of
// the name a capture saved now would get, expanded by the daemon exactly as
// a save expands it. A valid pattern is saved as it is typed.

import { useEffect, useRef, useState } from "react";
import { insertToken, patternProblem, TOKENS } from "./pattern";

export function PatternEditor(props: {
  value: string;
  extension: string;
  preview(pattern: string): Promise<string>;
  onChange(pattern: string): void;
  onProblem(problem: string | null): void;
}) {
  const [text, setText] = useState(props.value);
  const [shown, setShown] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const editing = useRef(false);
  const asked = useRef(0);

  useEffect(() => {
    if (!editing.current) setText(props.value);
  }, [props.value]);

  // The preview follows every keystroke; only the latest answer is shown
  useEffect(() => {
    const ticket = ++asked.current;
    props.preview(text).then(
      (name) => {
        if (ticket === asked.current) setShown(name);
      },
      () => {},
    );
  }, [text]);

  const change = (next: string) => {
    setText(next);
    const problem = patternProblem(next);
    props.onProblem(problem);
    if (!problem && next !== props.value) props.onChange(next);
  };

  return (
    <div className="pattern">
      <input
        ref={input}
        className="hex set-text-input mono"
        aria-label="File name pattern"
        data-testid="pattern-input"
        spellCheck={false}
        value={text}
        onFocus={() => {
          editing.current = true;
        }}
        onBlur={() => {
          editing.current = false;
          if (patternProblem(text)) {
            setText(props.value);
            props.onProblem(null);
          }
        }}
        onChange={(event) => change(event.target.value)}
      />
      <p className="pattern-preview" data-testid="pattern-preview" aria-live="polite">
        <span className="set-help">Preview</span>
        <span className="mono">
          {shown}.{props.extension || "png"}
        </span>
      </p>
      <div className="chips" role="group" aria-label="Insert into the pattern">
        {TOKENS.map(([label, token]) => (
          <button
            key={token}
            type="button"
            className="chip"
            title={token}
            onPointerDown={(event) => event.preventDefault()}
            onClick={() => {
              const field = input.current;
              const start = field?.selectionStart ?? text.length;
              const end = field?.selectionEnd ?? text.length;
              const next = insertToken(text, start, end, token);
              change(next.text);
              requestAnimationFrame(() => {
                field?.focus();
                field?.setSelectionRange(next.caret, next.caret);
              });
            }}
          >
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}
