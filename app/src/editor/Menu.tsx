// A small menu at a point (the filmstrip's right-click menu): one list of
// items, the arrow keys move between those that can be picked, Enter picks,
// Esc closes and gives the focus back. More items are more entries.

import { useEffect, useLayoutEffect, useRef, type KeyboardEvent as ReactKeyboardEvent } from "react";

export interface MenuItem {
  id: string;
  label: string;
  disabled?: boolean;
}

export interface MenuProps {
  label: string;
  items: readonly MenuItem[];
  /** Where it opens, in window coordinates; it stays inside the window. */
  at: { left: number; top: number };
  onPick(id: string): void;
  /** `restore`: Esc or Tab closed it, so the focus goes back to what
   * opened it. */
  onClose(restore: boolean): void;
}

/** Room kept between the menu and the window's edge, in CSS pixels. */
const EDGE = 8;

export function Menu({ label, items, at, onPick, onClose }: MenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const enabled = () => [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];

  // Placed before it paints: held inside the window, the first item focused
  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    menu.style.left = `${Math.max(EDGE, Math.min(at.left, window.innerWidth - menu.offsetWidth - EDGE))}px`;
    menu.style.top = `${Math.max(EDGE, Math.min(at.top, window.innerHeight - menu.offsetHeight - EDGE))}px`;
    (enabled()[0] ?? menu).focus();
  }, [at]);

  // A press anywhere else closes it
  useEffect(() => {
    const away = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose(false);
    };
    window.addEventListener("pointerdown", away);
    return () => window.removeEventListener("pointerdown", away);
  }, [onClose]);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const list = enabled();
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    const focus = (index: number) => list[(index + list.length) % list.length]?.focus();
    switch (event.key) {
      case "ArrowDown":
        focus(at + 1);
        break;
      case "ArrowUp":
        focus(at < 0 ? -1 : at - 1);
        break;
      case "Home":
        focus(0);
        break;
      case "End":
        focus(-1);
        break;
      case "Escape":
      case "Tab":
        onClose(true);
        break;
      case "Enter":
      case " ":
        // The focused item's own click picks it
        event.stopPropagation();
        return;
      default:
        return;
    }
    // The editor's own keys do not see these
    event.preventDefault();
    event.stopPropagation();
  };

  return (
    <div ref={ref} className="menu" role="menu" aria-label={label} tabIndex={-1} data-testid="menu" onKeyDown={onKeyDown}>
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="menuitem"
          className="menu-item"
          disabled={item.disabled}
          data-testid={`menu-${item.id}`}
          onClick={() => onPick(item.id)}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
