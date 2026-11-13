// The settings window: sections in a sidebar, a search box over all of
// them, and rows that save the moment they change. Raycast's rules apply
// (docs/design/raycast-notes.md): tokens only, feedback at once, no Apply.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { ColorField, PaletteEditor } from "./ColorEditors";
import { Keys, NumberField, Percent, Row, Select, Switch } from "./controls";
import { Icon } from "./Icon";
import { PatternEditor } from "./PatternEditor";
import { SECTIONS, type SectionId, type SettingDef } from "./schema";
import { ShortcutRecorder } from "./ShortcutRecorder";
import {
  displaced,
  hotkeyProblem,
  isDefault,
  printScreenProblem,
  search,
  settingsIn,
  withShortcut,
  withValue,
} from "./state";
import { ToolbarEditor } from "./ToolbarEditor";
import type { Backend } from "./backend";
import type { ShortcutRow, Snapshot, Value } from "./types";

const SHORTCUT_GROUPS = ["Global", "Tools", "Actions", "Capture", "Selection"];

type Messages = Record<string, string | undefined>;

export function SettingsApp({ initial, backend }: { initial: Snapshot; backend: Backend }) {
  const [view, setView] = useState(initial);
  const [section, setSection] = useState<SectionId>("general");
  const [query, setQuery] = useState("");
  const [errors, setErrors] = useState<Messages>({});
  const [notices, setNotices] = useState<Messages>({});
  const searchBox = useRef<HTMLInputElement>(null);
  // Writes go one at a time; a value changed again before its write
  // started is sent once, as its latest value
  const chain = useRef<Promise<void>>(Promise.resolve());
  const pending = useRef(new Map<string, Value>());

  const message = (id: string, error: string | null, notice: string | null = null) => {
    setErrors((all) => ({ ...all, [id]: error ?? undefined }));
    setNotices((all) => ({ ...all, [id]: notice ?? undefined }));
  };
  /** A problem found while typing; what the last change did stays shown. */
  const problemOf = (id: string, error: string | null) => setErrors((all) => ({ ...all, [id]: error ?? undefined }));

  /** The daemon's snapshot, with values still on their way shown as typed. */
  const accept = (next: Snapshot) => {
    let shown = next;
    for (const [key, value] of pending.current) shown = withValue(shown, key, value);
    setView(shown);
  };

  const queue = (job: () => Promise<void>) => {
    chain.current = chain.current.then(job, job);
  };

  const save = (key: string, value: Value) => {
    setView((current) => withValue(current, key, value));
    message(key, null);
    const queued = pending.current.has(key);
    pending.current.set(key, value);
    if (queued) return;
    queue(async () => {
      const latest = pending.current.get(key) as Value;
      pending.current.delete(key);
      try {
        accept(await backend.set(key, latest));
      } catch (error) {
        message(key, String(error));
        accept(await backend.snapshot());
      }
    });
  };

  /** Any other change: `job` answers with the new snapshot, or `null` for
   *  a cancelled dialog. */
  const run = (id: string, job: () => Promise<Snapshot | null>, done?: (before: Snapshot, after: Snapshot) => void) => {
    message(id, null);
    queue(async () => {
      try {
        const before = view;
        const next = await job();
        if (next) {
          accept(next);
          done?.(before, next);
        }
      } catch (error) {
        message(id, String(error));
      }
    });
  };

  // Changed elsewhere (phramer config, an import, another copy): shown here
  useEffect(() => {
    let release: (() => void) | undefined;
    let alive = true;
    void backend
      .onChanged(() => {
        if (pending.current.size === 0) void backend.snapshot().then((next) => alive && accept(next));
      })
      .then((unlisten) => {
        if (alive) release = unlisten;
        else unlisten();
      });
    return () => {
      alive = false;
      release?.();
    };
  }, [backend]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchBox.current?.focus();
        searchBox.current?.select();
      } else if (event.key === "Escape" && !event.defaultPrevented) {
        // 15.0.0's configuration window closed on Esc
        if (query) setQuery("");
        else void backend.close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [query, backend]);

  const palette = view.values.userColors as (string | null)[];

  const control = (def: SettingDef): { node: ReactNode; wide?: boolean } => {
    const value = view.values[def.key];
    const c = def.control;
    switch (c.kind) {
      case "switch": {
        const on = Boolean(value) !== Boolean(c.inverted);
        return {
          node: (
            <Switch
              checked={on}
              label={def.label}
              disabled={Boolean(def.unavailable?.(view))}
              onChange={(checked) => save(def.key, c.inverted ? !checked : checked)}
            />
          ),
        };
      }
      case "number":
        return {
          node: (
            <NumberField
              value={value as number}
              min={c.min}
              max={c.max}
              unit={c.unit}
              label={def.label}
              onChange={(next) => save(def.key, next)}
              onProblem={(problem) => problemOf(def.key, problem)}
            />
          ),
        };
      case "percent":
        return { node: <Percent value={value as number} max={c.max} label={def.label} onChange={(next) => save(def.key, next)} /> };
      case "select":
        return {
          node: <Select value={value as string | number} options={c.options} label={def.label} onChange={(next) => save(def.key, next)} />,
        };
      case "extension":
        return {
          node: (
            <Select
              value={value as string}
              options={view.extensions.map((extension) => [extension, extension.toUpperCase()] as const)}
              label={def.label}
              onChange={(next) => save(def.key, next)}
            />
          ),
        };
      case "folder":
        return {
          wide: true,
          node: (
            <div className="folder">
              <span className="folder-path mono" title={value as string} data-testid="save-path">
                {value as string}
              </span>
              <button type="button" className="text-button" onClick={() => run(def.key, () => backend.chooseFolder())}>
                <Icon name="folder-open" />
                Change…
              </button>
            </div>
          ),
        };
      case "pattern":
        return {
          wide: true,
          node: (
            <PatternEditor
              value={value as string}
              extension={view.values.saveAsFileExtension as string}
              preview={(pattern) => backend.previewPattern(pattern)}
              onChange={(next) => save(def.key, next)}
              onProblem={(problem) => problemOf(def.key, problem)}
            />
          ),
        };
      case "color":
        return {
          node: <ColorField value={value as string} label={def.label} palette={palette} onChange={(hex) => save(def.key, hex)} />,
        };
      case "palette":
        return {
          wide: true,
          node: (
            <PaletteEditor
              value={palette}
              onChange={(next) => save(def.key, next)}
              onProblem={(problem) => problemOf(def.key, problem)}
            />
          ),
        };
      case "toolbar":
        return {
          wide: true,
          node: <ToolbarEditor value={value as number[]} choices={view.toolbar} onChange={(ids) => save(def.key, ids)} />,
        };
      case "printscreen":
        return {
          node: (
            <Switch
              checked={!view.snipping_claims_print_screen}
              label={def.label}
              onChange={(checked) => run(def.key, () => backend.printScreen(checked))}
            />
          ),
        };
      case "file":
        return { wide: true, node: <FileTools snapshot={view} backend={backend} run={run} /> };
    }
  };

  const setting = (def: SettingDef) => {
    const { node, wide } = control(def);
    const resettable = def.key in view.defaults || def.key === "printScreen";
    return (
      <Row
        key={def.key}
        id={def.key}
        label={def.label}
        help={def.help}
        wide={wide}
        disabled={def.unavailable?.(view)}
        modified={!isDefault(view, def.key)}
        error={errors[def.key] ?? (def.key === "printScreen" ? printScreenProblem(view) : null)}
        notice={notices[def.key]}
        onReset={
          resettable
            ? () =>
                def.key === "printScreen"
                  ? run(def.key, () => backend.printScreen(false))
                  : run(def.key, () => backend.reset(def.key))
            : undefined
        }
      >
        {node}
      </Row>
    );
  };

  const shortcut = (row: ShortcutRow, index: number) => {
    const id = row.name ? `shortcut-${row.name}` : `fixed-${index}`;
    const record = (sequence: string) =>
      new Promise<string | null>((resolve) => {
        setView((current) => withShortcut(current, row.name, sequence));
        run(
          id,
          async () => {
            try {
              return await backend.setShortcut(row.name, sequence);
            } catch (error) {
              resolve(String(error));
              accept(await backend.snapshot());
              return null;
            }
          },
          (before, after) => {
            const lost = displaced(before, after, row.name);
            const changed = after.shortcuts.find((entry) => entry.name === row.name);
            const taken = changed ? hotkeyProblem(after, changed) : null;
            message(
              id,
              taken,
              lost.length > 0
                ? `Taken from “${lost.map((entry) => entry.description).join("”, “")}”, which now has no key`
                : null,
            );
            resolve(taken);
          },
        );
      });
    return (
      <Row
        key={id}
        id={id}
        label={row.description}
        modified={Boolean(row.name) && row.value !== row.default}
        error={errors[id] ?? hotkeyProblem(view, row)}
        notice={notices[id]}
        onReset={row.name ? () => void record(row.default) : undefined}
      >
        {row.name ? (
          <ShortcutRecorder
            sequence={row.value}
            label={row.description}
            global={row.global}
            onRecord={record}
            onListening={(listening) => run(id, () => backend.pauseHotkeys(listening))}
            onProblem={(problem) => problemOf(id, problem)}
          />
        ) : (
          <span className="fixed-key">
            <Keys sequence={row.value} />
          </span>
        )}
      </Row>
    );
  };

  const shortcutGroups = (rows: ShortcutRow[]) =>
    SHORTCUT_GROUPS.map((group) => {
      const inGroup = rows.filter((row) => row.group === group);
      if (inGroup.length === 0) return null;
      return (
        <section key={group} className="set-group" aria-label={group}>
          <h2 className="set-group-title">{group === "Global" ? "Anywhere in Windows" : group}</h2>
          {inGroup.map((row) => shortcut(row, rows.indexOf(row)))}
        </section>
      );
    });

  const printScreenBanner =
    view.snipping_claims_print_screen && !view.values.ignorePrntScrForcesSnipping ? (
      <div className="banner" role="note" data-testid="print-screen-banner">
        <span>Windows opens its Snipping Tool when you press Print Screen. Let Phramer use the key instead?</span>
        <div className="banner-actions">
          <button type="button" className="text-button primary" onClick={() => run("printScreen", () => backend.printScreen(true))}>
            Use Print Screen
          </button>
          <button type="button" className="text-button" onClick={() => save("ignorePrntScrForcesSnipping", true)}>
            Don't ask again
          </button>
        </div>
      </div>
    ) : null;

  const found = search(view, query);
  const searching = query.trim() !== "";
  const current = SECTIONS.find((entry) => entry.id === section) ?? SECTIONS[0];

  let body: ReactNode;
  if (searching) {
    const nothing = found.settings.length === 0 && found.shortcuts.length === 0;
    body = (
      <div className="section" key="search" data-testid="search-results">
        <h1 className="section-title">Results</h1>
        {nothing && <p className="empty">No settings match {"“"}{query.trim()}{"”"}.</p>}
        {SECTIONS.map((entry) => {
          const defs = found.settings.filter((def) => def.section === entry.id);
          if (defs.length === 0) return null;
          return (
            <section key={entry.id} className="set-group" aria-label={entry.title}>
              <h2 className="set-group-title">{entry.title}</h2>
              {defs.map(setting)}
            </section>
          );
        })}
        {found.shortcuts.length > 0 && (
          <section className="set-group" aria-label="Shortcuts">
            <h2 className="set-group-title">Shortcuts</h2>
            {found.shortcuts.map((row) => shortcut(row, view.shortcuts.indexOf(row)))}
          </section>
        )}
      </div>
    );
  } else {
    body = (
      <div className="section" key={section} data-testid={`section-${section}`}>
        <h1 className="section-title">{current.title}</h1>
        {section === "shortcuts" && printScreenBanner}
        <div className="set-group">{settingsIn(section).map(setting)}</div>
        {section === "shortcuts" && shortcutGroups(view.shortcuts)}
      </div>
    );
  }

  return (
    <div className="settings" data-testid="settings">
      <header className="titlebar" data-tauri-drag-region>
        <span className="title" data-tauri-drag-region>
          Phramer Settings
        </span>
        <div className="window-buttons">
          <button type="button" aria-label="Minimize" title="Minimize" onClick={() => void backend.minimize()}>
            <Icon name="minus" />
          </button>
          <button type="button" aria-label="Maximize" title="Maximize" onClick={() => void backend.toggleMaximize()}>
            <Icon name="square" />
          </button>
          <button type="button" className="close" aria-label="Close" title="Close (Esc)" onClick={() => void backend.close()}>
            <Icon name="x" />
          </button>
        </div>
      </header>
      <div className="settings-body">
        <nav className="sidebar" aria-label="Sections">
          <label className="search">
            <Icon name="search" />
            <input
              ref={searchBox}
              type="search"
              placeholder="Search settings"
              aria-label="Search settings"
              data-testid="search"
              spellCheck={false}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape" && query) {
                  event.preventDefault();
                  setQuery("");
                }
              }}
            />
          </label>
          <ul className="nav">
            {SECTIONS.map((entry) => (
              <li key={entry.id}>
                <button
                  type="button"
                  className="nav-item"
                  aria-current={!searching && entry.id === section ? "page" : undefined}
                  data-testid={`nav-${entry.id}`}
                  onClick={() => {
                    setQuery("");
                    setSection(entry.id);
                  }}
                >
                  <Icon name={entry.icon} />
                  {entry.title}
                </button>
              </li>
            ))}
          </ul>
          <p className="sidebar-foot">Phramer {view.version}</p>
        </nav>
        <main className="content" data-testid="content">
          {body}
        </main>
      </div>
    </div>
  );
}

/** 15.0.0's Configuration File box: Export, Import and Reset. */
function FileTools(props: {
  snapshot: Snapshot;
  backend: Backend;
  run(id: string, job: () => Promise<Snapshot | null>, done?: (before: Snapshot, after: Snapshot) => void): void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  useEffect(() => {
    if (!confirming) return;
    const timer = window.setTimeout(() => setConfirming(false), 4000);
    return () => window.clearTimeout(timer);
  }, [confirming]);
  return (
    <div className="file-tools">
      <span className="folder-path mono" title={props.snapshot.file}>
        {props.snapshot.file}
      </span>
      <div className="file-buttons">
        <button
          type="button"
          className="text-button"
          onClick={() =>
            props.run("settingsFile", async () => {
              const written = await props.backend.exportFile();
              setDone(written ? `Saved a copy to ${written}` : null);
              return null;
            })
          }
        >
          <Icon name="upload" />
          Export…
        </button>
        <button
          type="button"
          className="text-button"
          onClick={() =>
            props.run("settingsFile", async () => {
              const next = await props.backend.importFile();
              setDone(next ? "Imported and applied" : null);
              return next;
            })
          }
        >
          <Icon name="download" />
          Import…
        </button>
        <button
          type="button"
          className={`text-button${confirming ? " danger" : ""}`}
          data-testid="reset-all"
          onClick={() => {
            if (!confirming) {
              setConfirming(true);
              return;
            }
            setConfirming(false);
            props.run("settingsFile", async () => {
              const next = await props.backend.resetAll();
              setDone("Every setting but the shortcuts is back to its default");
              return next;
            });
          }}
        >
          <Icon name="rotate-ccw" />
          {confirming ? "Click again to reset everything" : "Reset all…"}
        </button>
      </div>
      {done && (
        <p className="set-help" role="status">
          {done}
        </p>
      )}
    </div>
  );
}
