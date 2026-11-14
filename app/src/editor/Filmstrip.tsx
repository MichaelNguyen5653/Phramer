// The strip under the image: a thumbnail per image (15.0.0's filmstrip,
// numbered from 1, marked when not saved) with its name under it, and what
// acts on the set: Save All and Remove Image. A double-click on a name
// edits it in place; a right-click on an image opens its menu (Rename, Move
// to front, Move to end).

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { FORBIDDEN_HINT, hasForbidden, withoutForbidden, type Doc, type Page } from "./images";
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
}

/** How long the hint about a refused character stays. */
const HINT_MS = 2500;

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
  };

  const menuIndex = menu ? pages.findIndex((page) => page.id === menu.id) : -1;
  const items: MenuItem[] = [
    { id: "rename", label: "Rename" },
    { id: "front", label: "Move to front", disabled: menuIndex <= 0 },
    { id: "end", label: "Move to end", disabled: menuIndex === pages.length - 1 },
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
                aria-label={`Image ${index + 1}, ${page.name}${unsaved ? ", not saved" : ""}`}
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
                  onDone={(typed, restore) => {
                    setEditing(null);
                    if (typed !== null) props.onRename(index, typed);
                    if (restore) refocus.current = page.id;
                  }}
                />
              ) : (
                <span
                  className={`thumb-name${index === props.current ? " active" : ""}`}
                  title={page.name}
                  data-testid={`name-${index + 1}`}
                  onClick={() => props.onShow(index)}
                  onDoubleClick={() => rename(page.id)}
                >
                  {page.name}
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
  /** `typed`: what to rename to, or `null` when cancelled. `restore`: a key
   * ended it, so the focus goes back to the thumbnail. */
  onDone(typed: string | null, restore: boolean): void;
}

/** A name edited in place: Enter or leaving keeps it, Esc puts it back.
 * Characters a file name cannot hold are refused as typed, with a hint. */
function NameField({ name, onDone }: NameFieldProps) {
  const [draft, setDraft] = useState(name);
  const [refused, setRefused] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  const timer = useRef<number | null>(null);

  useLayoutEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
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
  const refuse = () => {
    setRefused(true);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setRefused(false), HINT_MS);
  };

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
          if (hasForbidden(typed)) refuse();
          setDraft(withoutForbidden(typed));
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") finish(draft, true);
          else if (event.key === "Escape") finish(null, true);
          else if (event.key.length === 1 && hasForbidden(event.key)) refuse();
          else return;
          // Not typed, and not seen by the editor's keys
          event.preventDefault();
          event.stopPropagation();
        }}
        onBlur={() => finish(draft, false)}
      />
      {refused && (
        <span className="name-hint" role="status" data-testid="name-hint">
          {FORBIDDEN_HINT}
        </span>
      )}
    </>
  );
}
