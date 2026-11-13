// Records a key sequence for one shortcut. Click (or Enter) starts
// listening; the next key with its modifiers is saved at once. Esc cancels,
// Backspace clears. While it listens the daemon lets go of its hotkeys, so
// the keys of a hotkey reach the page instead of starting a capture.

import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { Keys } from "./controls";
import { heldModifiers, problem, sequenceFromEvent } from "./shortcut";

export function ShortcutRecorder(props: {
  sequence: string;
  label: string;
  global: boolean;
  /** Saves the sequence; resolves to why it was refused, or `null`. */
  onRecord(sequence: string): Promise<string | null>;
  onListening(listening: boolean): void;
  onProblem(problem: string | null): void;
}) {
  const [listening, setListening] = useState(false);
  const [held, setHeld] = useState<string[]>([]);
  const button = useRef<HTMLButtonElement>(null);
  const active = useRef(false);

  const stop = () => {
    if (!active.current) return;
    active.current = false;
    setListening(false);
    setHeld([]);
    props.onListening(false);
  };

  // A recorder unmounted while listening must give the hotkeys back
  useEffect(() => () => stop(), []);

  const start = () => {
    if (active.current) return;
    active.current = true;
    setListening(true);
    props.onProblem(null);
    props.onListening(true);
  };

  const submit = async (sequence: string) => {
    const refused = problem(sequence, props.global);
    if (refused) {
      props.onProblem(refused);
      return;
    }
    stop();
    props.onProblem(await props.onRecord(sequence));
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (!active.current) {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        start();
      }
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const plain = !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
    if (event.code === "Escape" && plain) {
      stop();
      props.onProblem(null);
      return;
    }
    if (event.code === "Backspace" && plain) {
      void submit("");
      return;
    }
    const sequence = sequenceFromEvent(event);
    if (sequence) void submit(sequence);
    else setHeld(heldModifiers(event));
  };

  return (
    <button
      ref={button}
      type="button"
      className={`recorder${listening ? " listening" : ""}`}
      aria-label={listening ? `Press the new keys for ${props.label}` : `Change the shortcut for ${props.label}`}
      aria-pressed={listening}
      onClick={() => (active.current ? stop() : start())}
      onKeyDown={onKeyDown}
      onKeyUp={(event) => {
        // Print Screen arrives as a key release only
        if (active.current && event.code === "PrintScreen") {
          event.preventDefault();
          const sequence = sequenceFromEvent(event);
          if (sequence) void submit(sequence);
        } else if (active.current) {
          setHeld(heldModifiers(event));
        }
      }}
      onBlur={() => stop()}
    >
      {listening ? (
        held.length > 0 ? (
          <Keys sequence={`${held.join("+")}+…`} />
        ) : (
          <span className="recorder-hint">Press keys…</span>
        )
      ) : (
        <Keys sequence={props.sequence} />
      )}
    </button>
  );
}
