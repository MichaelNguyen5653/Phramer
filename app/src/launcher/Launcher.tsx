// App Launcher: 15.0.0's "Open With" list of Start Menu apps, keyboard
// first. Type to filter, arrows to move, Enter to open the capture with the
// selected app, Esc to close. Windows' own "Open with" dialog is the last
// entry. The window closes after a choice unless "Keep open after
// selection" is on.

import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { AppWindow, ExternalLink, Search, X } from "lucide-react";
import type { Backend, LauncherInit } from "./backend";
import { filterApps, moveSelection, type AppEntry } from "./filter";

/** Icons asked for per call, so the first rows get theirs soonest. */
const ICON_BATCH = 24;

type Row = { kind: "app"; app: AppEntry } | { kind: "open-with" };

export function Launcher({ initial, backend }: { initial: LauncherInit; backend: Backend }) {
  const [apps, setApps] = useState<AppEntry[] | null>(null);
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [keepOpen, setKeepOpen] = useState(initial.keep_open);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const list = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let live = true;
    backend
      .apps()
      .then(async (found) => {
        if (!live) return;
        setApps(found);
        const exes = [...new Set(found.map((app) => app.exe))];
        for (let start = 0; start < exes.length && live; start += ICON_BATCH) {
          const batch = await backend.icons(exes.slice(start, start + ICON_BATCH)).catch(() => ({}));
          if (live) setIcons((current) => ({ ...current, ...batch }));
        }
      })
      .catch((problem) => {
        if (!live) return;
        setApps([]);
        setMessage({ text: String(problem), error: true });
      });
    return () => {
      live = false;
    };
  }, [backend]);

  const shown = useMemo(() => filterApps(apps ?? [], query), [apps, query]);
  const rows: Row[] = useMemo(() => [...shown.map((app) => ({ kind: "app" as const, app })), { kind: "open-with" as const }], [shown]);
  const current = Math.min(selected, rows.length - 1);

  useEffect(() => {
    list.current?.querySelector(`[data-index="${current}"]`)?.scrollIntoView({ block: "nearest" });
  }, [current]);

  const done = (text: string) => {
    if (keepOpen) setMessage({ text, error: false });
    else void backend.close();
  };

  const choose = (row: Row) => {
    setMessage(null);
    const job = row.kind === "app" ? backend.launch(row.app.exe) : backend.openWith();
    job
      .then(() => done(row.kind === "app" ? `Opened in ${row.app.name}.` : "Choose an app in the Open with dialog."))
      .catch((problem) => setMessage({ text: String(problem), error: true }));
  };

  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void backend.close();
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(rows[current]);
    } else if (["ArrowDown", "ArrowUp", "PageDown", "PageUp"].includes(event.key)) {
      event.preventDefault();
      setSelected(moveSelection(current, event.key, rows.length));
    }
  };

  const toggleKeepOpen = () => {
    const on = !keepOpen;
    setKeepOpen(on);
    backend.setKeepOpen(on).catch((problem) => setMessage({ text: String(problem), error: true }));
  };

  const firstUtility = shown.findIndex((app) => !app.graphics);

  return (
    <div className="launcher" data-testid="launcher" onKeyDown={onKey}>
      <header className="panel-titlebar" data-tauri-drag-region>
        <span className="panel-title" data-tauri-drag-region>
          Open With
          <span className="panel-title-detail" data-tauri-drag-region>
            {initial.file_name}
          </span>
        </span>
        <button type="button" className="panel-close" aria-label="Close" title="Close (Esc)" onClick={() => void backend.close()}>
          <X className="icon" aria-hidden="true" />
        </button>
      </header>

      <div className="launcher-search">
        <Search className="icon" aria-hidden="true" />
        <input
          ref={search}
          type="search"
          autoFocus
          placeholder="Search apps"
          aria-label="Search apps"
          aria-controls="launcher-list"
          aria-activedescendant={`launcher-row-${current}`}
          data-testid="launcher-search"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setSelected(0);
          }}
        />
      </div>

      <div className="launcher-list" id="launcher-list" role="listbox" aria-label="Apps" ref={list} data-testid="launcher-list">
        {apps === null && (
          <div className="launcher-loading" role="status">
            <span className="spinner" aria-hidden="true" />
            Reading the Start Menu{"…"}
          </div>
        )}
        {apps !== null && shown.length === 0 && (
          <p className="launcher-empty">{query ? `No apps match “${query}”` : "No apps found in the Start Menu"}</p>
        )}
        {rows.map((row, index) => {
          const isSelected = index === current;
          const section =
            row.kind === "open-with"
              ? "Other"
              : index === 0 && row.app.graphics
                ? "Graphics"
                : index === firstUtility
                  ? "Utility"
                  : null;
          return (
            <div key={row.kind === "app" ? `${row.app.exe}|${row.app.name}` : "open-with"}>
              {section && <div className="launcher-section">{section}</div>}
              <div
                id={`launcher-row-${index}`}
                role="option"
                aria-selected={isSelected}
                data-index={index}
                data-testid={row.kind === "app" ? "launcher-app" : "launcher-open-with"}
                className={`launcher-row${isSelected ? " selected" : ""}`}
                title={row.kind === "app" ? row.app.exe : undefined}
                onMouseMove={() => index !== current && setSelected(index)}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => choose(row)}
              >
                {row.kind === "app" ? (
                  icons[row.app.exe] ? (
                    <img className="launcher-icon" src={icons[row.app.exe]} alt="" />
                  ) : (
                    <span className="launcher-icon placeholder">
                      <AppWindow className="icon" aria-hidden="true" />
                    </span>
                  )
                ) : (
                  <span className="launcher-icon placeholder">
                    <ExternalLink className="icon" aria-hidden="true" />
                  </span>
                )}
                <span className="launcher-name">{row.kind === "app" ? row.app.name : "Open with…"}</span>
                {row.kind === "open-with" && <span className="launcher-subtitle">Choose another app</span>}
                {isSelected && <kbd className="launcher-enter">Enter</kbd>}
              </div>
            </div>
          );
        })}
      </div>

      <footer className="launcher-foot">
        <label className="launcher-keep" data-testid="launcher-keep-open">
          <button type="button" role="switch" aria-checked={keepOpen} className="switch" aria-label="Keep open after selection" onClick={toggleKeepOpen}>
            <span className="switch-thumb" />
          </button>
          Keep open after selection
        </label>
        {message ? (
          <p className={`message${message.error ? " error" : ""}`} role={message.error ? "alert" : "status"} data-testid="launcher-message">
            {message.text}
          </p>
        ) : (
          <span className="launcher-keys" aria-hidden="true">
            <kbd>{"↑"}</kbd>
            <kbd>{"↓"}</kbd> to move <kbd>Esc</kbd> to close
          </span>
        )}
      </footer>
    </div>
  );
}
