// The settings rows and their small controls. Everything a row changes is
// saved at once; there is no OK or Apply anywhere.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Icon } from "./Icon";
import { keycaps } from "./shortcut";
import { parseWhole } from "./validate";

export function Row(props: {
  id: string;
  label: string;
  help?: string;
  /** Shown under the row: a refused value, or what a change did. */
  error?: string | null;
  notice?: string | null;
  modified: boolean;
  onReset?: () => void;
  /** Controls that need the row's whole width go under the label. */
  wide?: boolean;
  disabled?: string | null;
  children: ReactNode;
}) {
  const labelId = `label-${props.id}`;
  return (
    <div
      className={`set-row${props.wide ? " wide" : ""}${props.disabled ? " disabled" : ""}`}
      data-testid={`row-${props.id}`}
      role="group"
      aria-labelledby={labelId}
    >
      <div className="set-text">
        <span className="set-label" id={labelId}>
          {props.label}
        </span>
        {props.help && <span className="set-help">{props.help}</span>}
        {props.disabled && <span className="set-help">{props.disabled}</span>}
      </div>
      <div className="set-control">
        {props.onReset && (
          <button
            type="button"
            className={`icon-button set-reset${props.modified ? "" : " hidden"}`}
            aria-label={`Reset ${props.label} to its default`}
            title="Reset to default"
            tabIndex={props.modified ? 0 : -1}
            aria-hidden={!props.modified}
            data-testid={`reset-${props.id}`}
            onClick={props.onReset}
          >
            <Icon name="rotate-ccw" />
          </button>
        )}
        {props.children}
      </div>
      {(props.error || props.notice) && (
        <p className={`set-message${props.error ? " error" : ""}`} role={props.error ? "alert" : "status"}>
          {props.error ?? props.notice}
        </p>
      )}
    </div>
  );
}

export function Switch(props: { checked: boolean; label: string; disabled?: boolean; onChange(checked: boolean): void }) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      disabled={props.disabled}
      onClick={() => props.onChange(!props.checked)}
    >
      <span className="switch-thumb" />
    </button>
  );
}

/** A whole number, saved as soon as what is typed is valid. */
export function NumberField(props: {
  value: number;
  min: number;
  max: number;
  unit?: string;
  label: string;
  onChange(value: number): void;
  onProblem(problem: string | null): void;
}) {
  const [text, setText] = useState(String(props.value));
  const editing = useRef(false);
  useEffect(() => {
    if (!editing.current) setText(String(props.value));
  }, [props.value]);
  return (
    <span className="set-number">
      <input
        className="number"
        inputMode="numeric"
        aria-label={props.label}
        value={text}
        onFocus={() => {
          editing.current = true;
        }}
        onBlur={() => {
          editing.current = false;
          setText(String(props.value));
          props.onProblem(null);
        }}
        onChange={(event) => {
          setText(event.target.value);
          const parsed = parseWhole(event.target.value, props.min, props.max);
          if ("error" in parsed) {
            props.onProblem(parsed.error);
          } else {
            props.onProblem(null);
            if (parsed.value !== props.value) props.onChange(parsed.value);
          }
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
      {props.unit && <span className="set-unit">{props.unit}</span>}
    </span>
  );
}

export function Select<T extends string | number>(props: {
  value: T;
  options: readonly (readonly [T, string])[];
  label: string;
  onChange(value: T): void;
}) {
  return (
    <select
      className="set-select"
      aria-label={props.label}
      value={String(props.value)}
      onChange={(event) => {
        const chosen = props.options.find(([value]) => String(value) === event.target.value);
        if (chosen) props.onChange(chosen[0]);
      }}
    >
      {props.options.map(([value, text]) => (
        <option key={String(value)} value={String(value)}>
          {text}
        </option>
      ))}
    </select>
  );
}

/** `0..max` stored, shown as a percentage. */
export function Percent(props: { value: number; max: number; label: string; onChange(value: number): void }) {
  const percent = Math.round((props.value / props.max) * 100);
  return (
    <span className="set-percent">
      <input
        type="range"
        min={0}
        max={100}
        value={percent}
        aria-label={props.label}
        onChange={(event) => props.onChange(Math.round((Number(event.target.value) / 100) * props.max))}
      />
      <span className="set-unit mono">{percent}%</span>
    </span>
  );
}

export function Keys({ sequence }: { sequence: string }) {
  const caps = keycaps(sequence);
  if (caps.length === 0) return <span className="set-none">None</span>;
  return (
    <span className="keys">
      {caps.map((cap, index) => (
        <kbd key={index}>{cap}</kbd>
      ))}
    </span>
  );
}
