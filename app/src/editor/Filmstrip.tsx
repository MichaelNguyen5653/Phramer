// The strip under the image: a thumbnail per image (15.0.0's filmstrip,
// numbered from 1, marked when not saved) with its file name under it, and
// what acts on the set: Save All and Remove Image. A double-click on a name
// edits it in place; a right-click on an image opens its menu (Rename, Move
// to front, Move to end, Convert to…).

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { FORMATS, FORMATS_HINT, formatOf, type Format } from "../formats";
import { FORBIDDEN_HINT, fileName, hasForbidden, parseName, withoutForbidden, type Doc, type Page } from "./images";
import { Icon } from "./Icon";
import { Menu, type MenuItem } from "./Menu";

export interface FilmstripProps<D extends Doc> {
  pages: readonly Page<D>[];
  current: number;
  savingAll: boolean;
  onShow(index: number): void;
  onSaveAll(): void;
  onRemove(): void;
  /** Before a name is edited or the menu opens: text typed on the image is
   * placed, so the keys reach the strip. */
  onBeforeEdit(): void;
  onRename(index: number, typed: string): void;
  onMoveToFront(index: number): void;
  onMoveToEnd(index: number): void;
  onConvert(index: number, format: Format): void;
}

/** How long the hint about a refused character stays. */
const HINT_MS = 2500;
/** The menu's Convert to… items: `convert-<format>`. */
const CONVERT = "convert-";

export function Filmstrip<D extends Doc>(props: FilmstripProps<D>) {
  const { pages, onBeforeEdit } = props;
  /** The image (by id) whose name is being edited. */
  const [editing, setEditing] = useState<number | null>(null);
  const [menu, setMenu] = useState<{ id: number; at: { left: number; top: number } } | null>(null);
  const thumbs = useRef(new Map<number, HTMLButtonElement>());
  /** The image whose thumbnail takes the focus once the strip has drawn. */
  const refocus = useRef<number | null>(null);

  useLayoutEffect(() => {
    if (refocus.current === null) return;
    thumbs.current.get(refocus.current)?.focus();
    refocus.current = null;
  });

  const rename = useCallback(
    (id: number) => {
      onBeforeEdit();
      setEditing(id);
    },
    [onBeforeEdit],
  );

  const openMenu = (event: ReactMouseEvent, page: Page<D>) => {
    event.preventDefault();
    onBeforeEdit();
    let at = { left: event.clientX, top: event.clientY };
    // From the keyboard (the Menu key, Shift+F10): under the thumbnail
    if (at.left === 0 && at.top === 0) {
      const box = thumbs.current.get(page.id)?.getBoundingClientRect();
      if (box) at = { left: box.left, top: box.bottom };
    }
    setMenu({ id: page.id, at });
  };

  const closeMenu = useCallback((restore: boolean) => {
    setMenu((open) => {
      if (open && restore) refocus.current = open.id;
      return null;
    });
  }, []);

  const pick = (id: string) => {
    if (!menu) return;
    const index = pages.findIndex((page) => page.id === menu.id);
    setMenu(null);
    if (index < 0) return;
    if (id === "rename") {
      rename(menu.id);
      return;
    }
    refocus.current = menu.id;
    if (id === "front") props.onMoveToFront(index);
    if (id === "end") props.onMoveToEnd(index);
    const format = id.startsWith(CONVERT) ? formatOf(id.slice(CONVERT.length)) : null;
    if (format) props.onConvert(index, format);
  };

  const menuIndex = menu ? pages.findIndex((page) => page.id === menu.id) : -1;
  const menuFormat = pages[menuIndex]?.format;
  const items: MenuItem[] = [
    { id: "rename", label: "Rename" },
    { id: "front", label: "Move to front", disabled: menuIndex <= 0 },
    { id: "end", label: "Move to end", disabled: menuIndex === pages.length - 1 },
    // The current format is marked and cannot be picked
    ...FORMATS.map((format) => ({
      id: `${CONVERT}${format.id}`,
      label: format.label,
      group: "Convert to…",
      checked: format.id === menuFormat,
      disabled: format.id === menuFormat,
    })),
  ];

  return (
    <section className="filmstrip" aria-label="Images" data-testid="filmstrip">
      <div className="filmstrip-actions">
        <button
          type="button"
          className="text-button"
          title="Save every image in this session to one folder"
          disabled={props.savingAll}
          data-testid="save-all"
          onClick={props.onSaveAll}
        >
          <Icon name="folder-down" />
          Save All
        </button>
        <button
          type="button"
          className="text-button"
          title="Remove the current image from this editor session"
          data-testid="remove-image"
          onClick={props.onRemove}
        >
          <Icon name="trash-2" />
          Remove Image
        </button>
      </div>
      <ol className="thumbs">
        {pages.map((page, index) => {
          const unsaved = page.doc.dirty;
          const file = fileName(page);
          return (
            <li key={page.id} onContextMenu={(event) => openMenu(event, page)}>
              <button
                ref={(button) => {
                  if (button) thumbs.current.set(page.id, button);
                  else thumbs.current.delete(page.id);
                }}
                type="button"
                className={`thumb${index === props.current ? " active" : ""}`}
                aria-current={index === props.current}
                aria-label={`Image ${index + 1}, ${file}${unsaved ? ", not saved" : ""}`}
                title={`Image ${index + 1}${unsaved ? " (not saved)" : ""}`}
                data-testid={`thumb-${index + 1}`}
                onClick={() => props.onShow(index)}
              >
                {page.thumbnail && <img src={page.thumbnail} alt="" draggable={false} />}
                <span className="thumb-number">{index + 1}</span>
                {unsaved && <span className="thumb-unsaved" data-testid="unsaved" aria-hidden="true" />}
              </button>
              {editing === page.id ? (
                <NameField
                  name={page.name}
                  format={page.format}
                  onDone={(typed, restore) => {
                    setEditing(null);
                    if (typed !== null) props.onRename(index, typed);
                    if (restore) refocus.current = page.id;
                  }}
                />
              ) : (
                <span
                  className={`thumb-name${index === props.current ? " active" : ""}`}
                  title={file}
                  data-testid={`name-${index + 1}`}
                  onClick={() => props.onShow(index)}
                  onDoubleClick={() => rename(page.id)}
                >
                  {/* A long name is cut short; its extension always shows */}
                  <span className="thumb-stem">{page.name}</span>
                  <span className="thumb-ext">.{page.format}</span>
                </span>
              )}
            </li>
          );
        })}
      </ol>
      {menu && <Menu label="Image" items={items} at={menu.at} onPick={pick} onClose={closeMenu} />}
    </section>
  );
}

interface NameFieldProps {
  name: string;
  format: Format;
  /** `typed`: what to rename to, or `null` when cancelled. `restore`: a key
   * ended it, so the focus goes back to the thumbnail. */
  onDone(typed: string | null, restore: boolean): void;
}

/** A name edited in place, with its extension: Enter or leaving keeps it,
 * Esc puts it back. Characters a file name cannot hold are refused as
 * typed, and the extension of a format Phramer does not save when it is
 * kept, each with a hint. */
function NameField({ name, format, onDone }: NameFieldProps) {
  const [draft, setDraft] = useState(`${name}.${format}`);
  const [hint, setHint] = useState<string | null>(null);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  const timer = useRef<number | null>(null);

  // The name is selected, not its extension, as Explorer does
  useLayoutEffect(() => {
    ref.current?.focus();
    ref.current?.setSelectionRange(0, name.length);
  }, [name]);
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const finish = (typed: string | null, restore: boolean) => {
    if (done.current) return;
    done.current = true;
    onDone(typed, restore);
  };
  const refuse = (text: string) => {
    setHint(text);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setHint(null), HINT_MS);
  };
  /** `.tif` and the like: not kept, and said why. */
  const refusedFormat = (typed: string) => "refused" in parseName(typed);

  return (
    <>
      <input
        ref={ref}
        className="thumb-name-input"
        aria-label="Image name"
        spellCheck={false}
        value={draft}
        data-testid="name-input"
        onChange={(event) => {
          // Pasted text: what a name can hold of it
          const typed = event.target.value;
          if (hasForbidden(typed)) refuse(FORBIDDEN_HINT);
          setDraft(withoutForbidden(typed));
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            // A format Phramer does not save: the field stays open
            if (refusedFormat(draft)) refuse(FORMATS_HINT);
            else finish(draft, true);
          } else if (event.key === "Escape") finish(null, true);
          else if (event.key.length === 1 && hasForbidden(event.key)) refuse(FORBIDDEN_HINT);
          else return;
          // Not typed, and not seen by the editor's keys
          event.preventDefault();
          event.stopPropagation();
        }}
        // Leaving with a refused format keeps the name as it was
        onBlur={() => finish(refusedFormat(draft) ? null : draft, false)}
      />
      {hint && (
        <span className="name-hint" role="status" data-testid="name-hint">
          {hint}
        </span>
      )}
    </>
  );
}
