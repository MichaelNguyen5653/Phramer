// Capture Launcher: 15.0.0's dialog for a capture with options. A mode, a
// monitor (only with more than one), the area to start with (Rectangular
// Region only; the last capture's by default), a delay counted down in the
// window, and "Take new screenshot", which has the focus so Enter starts.

import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { X } from "lucide-react";
import type { Area, Backend, CaptureLauncherInit, Mode } from "./backend";

const MAX_DELAY = 999;
const NO_AREA: Area = { x: 0, y: 0, width: 0, height: 0 };

/** A whole number from a field, 0 when it holds none. */
function whole(text: string, min: number, max: number): number {
  const value = Number.parseInt(text, 10);
  return Number.isNaN(value) ? 0 : Math.min(max, Math.max(min, value));
}

export function CaptureLauncher({ initial, backend }: { initial: CaptureLauncherInit; backend: Backend }) {
  const [mode, setMode] = useState<Mode>("region");
  const [monitor, setMonitor] = useState(initial.current);
  const [area, setArea] = useState<Area>(initial.last_region ?? NO_AREA);
  const [delay, setDelay] = useState(0);
  const [taking, setTaking] = useState(false);
  const [remaining, setRemaining] = useState<number | null>(null);
  const take = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const stop = backend.onBack(() => {
      setTaking(false);
      setRemaining(null);
    });
    return () => {
      void stop.then((unlisten) => unlisten());
    };
  }, [backend]);

  // At the start, and enabled again after an aborted capture
  useEffect(() => {
    if (!taking) take.current?.focus();
  }, [taking]);

  // The countdown; at zero the window goes, ahead of the capture
  useEffect(() => {
    if (remaining === null) return;
    if (remaining <= 0) {
      setRemaining(null);
      void backend.hide();
      return;
    }
    const timer = setTimeout(() => setRemaining(remaining - 1), 1000);
    return () => clearTimeout(timer);
  }, [backend, remaining]);

  const start = () => {
    setTaking(true);
    if (delay > 0) setRemaining(delay);
    void backend.take({ mode, monitor, region: area, delay_s: delay });
  };

  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void backend.close();
    }
  };

  const areaField = (key: keyof Area, label: string, min: number) => (
    <label className="capture-area-field">
      <span className="capture-area-name">{label}</span>
      <input
        type="number"
        className="field-select"
        aria-label={label}
        min={min}
        value={area[key]}
        disabled={taking}
        onChange={(event) => setArea({ ...area, [key]: whole(event.target.value, min, 99999) })}
      />
    </label>
  );

  return (
    <div className="capture-launcher" data-testid="capture-launcher" onKeyDown={onKey}>
      <header className="panel-titlebar" data-tauri-drag-region>
        <span className="panel-title" data-tauri-drag-region>
          Capture Launcher
        </span>
        <button type="button" className="panel-close" aria-label="Close" title="Close (Esc)" onClick={() => void backend.close()}>
          <X className="icon" aria-hidden="true" />
        </button>
      </header>

      <div className="capture-form">
        <label className="capture-row">
          <span className="capture-label">Mode</span>
          <select
            className="field-select"
            aria-label="Capture mode"
            value={mode}
            disabled={taking}
            onChange={(event) => setMode(event.target.value as Mode)}
          >
            <option value="region">Rectangular Region</option>
            <option value="full">Full Screen</option>
          </select>
        </label>

        {initial.monitors.length > 1 && (
          <label className="capture-row">
            <span className="capture-label">Monitor</span>
            <select
              className="field-select"
              aria-label="Monitor"
              value={monitor}
              disabled={taking}
              onChange={(event) => setMonitor(Number(event.target.value))}
            >
              {initial.monitors.map((entry, index) => (
                <option key={index} value={index}>
                  {`Monitor ${index + 1}: ${entry.name} (${entry.width}x${entry.height})`}
                </option>
              ))}
            </select>
          </label>
        )}

        {mode === "region" && (
          <div className="capture-row" role="group" aria-label="Area">
            <span className="capture-label">Area</span>
            <span className="capture-area">
              {areaField("x", "X", -99999)}
              {areaField("y", "Y", -99999)}
              {areaField("width", "Width", 0)}
              {areaField("height", "Height", 0)}
            </span>
          </div>
        )}

        <label className="capture-row">
          <span className="capture-label">Delay</span>
          <span className="capture-delay">
            <input
              type="number"
              className="field-select"
              aria-label="Delay"
              min={0}
              max={MAX_DELAY}
              value={delay}
              disabled={taking}
              onChange={(event) => setDelay(whole(event.target.value, 0, MAX_DELAY))}
            />
            <span className="capture-unit" data-testid="delay-unit">
              {delay === 0 ? "No delay" : delay === 1 ? "second" : "seconds"}
            </span>
          </span>
        </label>
      </div>

      <footer className="capture-foot">
        {remaining !== null && (
          <span className="capture-countdown" role="timer" aria-live="polite" data-testid="countdown">
            {remaining}
          </span>
        )}
        <button ref={take} type="button" className="button primary" disabled={taking} onClick={start}>
          Take new screenshot
        </button>
      </footer>
    </div>
  );
}
