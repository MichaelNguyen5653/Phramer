// The editor's tools and actions: the overlay's toolbar (same order, same
// Lucide icons, same `[Shortcuts]` entries; phramer-overlay/src/state/tools.rs)
// and the rules it draws by (phramer-overlay/src/state/annotate.rs).

import type { ArrowHead, Point, RedactStyle, Shape, ShapeStyle, TextAlign } from "./engine";

export type ToolId =
  | "pencil"
  | "line"
  | "arrow"
  | "shape"
  | "selection"
  | "rectangle"
  | "circle"
  | "marker"
  | "text"
  | "circle_count"
  | "pixelate"
  | "invert"
  | "move_object";

export type ActionId =
  | "undo"
  | "redo"
  | "copy"
  | "copy_file"
  | "save"
  | "save_as"
  | "close"
  | "open_app"
  | "ocr"
  | "pin"
  | "size_increase"
  | "size_decrease"
  | "crop"
  | "zoom_in"
  | "zoom_out"
  | "fit"
  | "actual_size"
  | "delete"
  | "commit";

export interface ButtonInfo<T> {
  id: T;
  name: string;
  description: string;
  /** Lucide icon name (phramer-overlay's `ICONS`). */
  icon: string;
  /** The `[Shortcuts]` entry, if it has one. */
  shortcut?: string;
}

/** The overlay's tools, in its toolbar order. Move Selection has no
 * meaning without a selection area and is left out. */
export const TOOLS: ButtonInfo<ToolId>[] = [
  { id: "pencil", name: "Pencil", description: "Set the Pencil as the paint tool", icon: "pencil", shortcut: "TYPE_PENCIL" },
  { id: "line", name: "Line", description: "Set the Line as the paint tool", icon: "slash", shortcut: "TYPE_DRAWER" },
  { id: "arrow", name: "Arrow", description: "Set the Arrow as the paint tool", icon: "arrow-up-right", shortcut: "TYPE_ARROW" },
  { id: "shape", name: "Shape", description: "Draw a square or circle, hollow, filled or as a highlight", icon: "shapes", shortcut: "TYPE_SHAPE" },
  { id: "selection", name: "Rectangular Selection", description: "Set Selection as the paint tool", icon: "square", shortcut: "TYPE_SELECTION" },
  { id: "rectangle", name: "Rectangle", description: "Set the Rectangle as the paint tool", icon: "rectangle-horizontal", shortcut: "TYPE_RECTANGLE" },
  { id: "circle", name: "Circle", description: "Set the Circle as the paint tool", icon: "circle", shortcut: "TYPE_CIRCLE" },
  { id: "marker", name: "Marker", description: "Set the Marker as the paint tool", icon: "highlighter", shortcut: "TYPE_MARKER" },
  { id: "text", name: "Text", description: "Add text to your capture", icon: "type", shortcut: "TYPE_TEXT" },
  { id: "circle_count", name: "Circle Counter", description: "Add an autoincrementing counter bubble", icon: "hash", shortcut: "TYPE_CIRCLECOUNT" },
  { id: "pixelate", name: "Pixelate", description: "Set Pixelate as the paint tool.", icon: "grid-3x3", shortcut: "TYPE_PIXELATE" },
  { id: "invert", name: "Invert", description: "Set Inverter as the paint tool", icon: "contrast", shortcut: "TYPE_INVERT" },
  { id: "move_object", name: "Move Object", description: "Select and move objects you have already drawn", icon: "hand", shortcut: "TYPE_MOVE_OBJECT" },
];

/** The overlay's actions that apply to an editor, then the editor's own. */
export const ACTIONS: ButtonInfo<ActionId>[] = [
  { id: "undo", name: "Undo", description: "Undo the last modification", icon: "undo-2", shortcut: "TYPE_UNDO" },
  { id: "redo", name: "Redo", description: "Redo the next modification", icon: "redo-2", shortcut: "TYPE_REDO" },
  { id: "copy", name: "Copy", description: "Copy the image to the clipboard", icon: "copy", shortcut: "TYPE_COPY" },
  { id: "copy_file", name: "Copy as File", description: "Save the image and copy the file, to paste as an attachment", icon: "file-image", shortcut: "TYPE_COPY_FILE" },
  { id: "save", name: "Save", description: "Save the image to the save folder", icon: "save", shortcut: "TYPE_SAVE" },
  { id: "save_as", name: "Save As", description: "Save the image where you choose", icon: "save-all" },
  { id: "open_app", name: "App Launcher", description: "Choose an app to open the image", icon: "external-link", shortcut: "TYPE_OPEN_APP" },
  { id: "ocr", name: "OCR", description: "Extract text from the image", icon: "scan-text", shortcut: "TYPE_OCR" },
  { id: "pin", name: "Pin Tool", description: "Pin the image on the desktop", icon: "pin", shortcut: "TYPE_PIN" },
  { id: "size_increase", name: "Increase Tool Size", description: "Increase the size of the other tools", icon: "plus", shortcut: "TYPE_SIZEINCREASE" },
  { id: "size_decrease", name: "Decrease Tool Size", description: "Decrease the size of the other tools", icon: "minus", shortcut: "TYPE_SIZEDECREASE" },
  { id: "crop", name: "Crop", description: "Crop the image (annotations are kept in the pixels)", icon: "crop" },
  { id: "zoom_out", name: "Zoom Out", description: "Zoom out", icon: "zoom-out" },
  { id: "zoom_in", name: "Zoom In", description: "Zoom in", icon: "zoom-in" },
  { id: "fit", name: "Fit", description: "Fit the image in the window", icon: "scan" },
  { id: "actual_size", name: "100%", description: "Show the image at its actual size", icon: "square-dashed" },
  { id: "close", name: "Close", description: "Close the editor", icon: "x", shortcut: "TYPE_EXIT" },
];

/** Keys of the editor that are not in `[Shortcuts]`. */
export const FIXED_SHORTCUTS: [string, ActionId][] = [
  ["Ctrl+Shift+S", "save_as"],
  ["Ctrl+Y", "redo"],
  ["Ctrl+=", "zoom_in"],
  ["Ctrl++", "zoom_in"],
  ["Ctrl+-", "zoom_out"],
  ["Ctrl+0", "fit"],
  ["Ctrl+1", "actual_size"],
  ["Ctrl+W", "close"],
  ["Backspace", "delete"],
];

/** `MAX_TOOL_SIZE`. */
export const MAX_TOOL_SIZE = 50;

export function isDrawing(tool: ToolId | null): tool is Exclude<ToolId, "move_object"> {
  return tool !== null && tool !== "move_object";
}

/** The engine tool a button places, for its Shift constraint (`kind_of`). */
export function engineKind(tool: ToolId): Shape["tool"] {
  switch (tool) {
    case "pencil": return "pencil";
    case "line": return "line";
    case "arrow": return "arrow";
    case "circle": return "ellipse";
    case "marker": return "marker";
    case "text": return "text";
    case "circle_count": return "circle_counter";
    case "pixelate": return "pixelate";
    case "invert": return "invert";
    default: return "rectangle";
  }
}

export type ShapeKind = "square" | "circle";

/** The tool options that shape what a tool draws. */
export interface ToolOptions {
  arrowHead: ArrowHead;
  reverseArrow: boolean;
  shapeKind: ShapeKind;
  shapeStyle: ShapeStyle;
  /** `rectangleFillMode`: filled or highlight. */
  rectangleStyle: ShapeStyle;
  highlightOutline: number;
  redact: RedactStyle;
  textAlign: TextAlign;
  underline: boolean;
  strikeout: boolean;
}

/** A tool size (logical, as the settings hold it) as an object's `size`
 * at `scale` (`OverlayState::object_size`). */
export function objectSize(kind: Shape["tool"], size: number, scale: number): number {
  const scaled = (base: number) => Math.round((size + base) * scale);
  switch (kind) {
    case "text": return Math.max(0, scaled(8) - 8);
    case "circle_counter": return Math.max(0, scaled(15) - 15);
    case "marker": return scaled(14);
    case "pixelate":
    case "invert": return size;
    default: return Math.max(1, scaled(0));
  }
}

/** What `tool` draws between two points (`shape_between`). */
export function shapeBetween(
  tool: ToolId,
  start: Point,
  end: Point,
  options: ToolOptions,
  scale: number,
  nextCounter: number,
): Shape | null {
  const outline_width = Math.round(options.highlightOutline * scale);
  const closed = (kind: ShapeKind, style: ShapeStyle): Shape =>
    kind === "square"
      ? { tool: "rectangle", start, end, style, outline_width }
      : { tool: "ellipse", start, end, style, outline_width };
  switch (tool) {
    case "pencil": return { tool: "pencil", points: [start, end] };
    case "line": return { tool: "line", start, end };
    case "arrow": return { tool: "arrow", start, end, head: options.arrowHead, reversed: options.reverseArrow };
    case "shape": return closed(options.shapeKind, options.shapeStyle);
    case "selection": return closed("square", "outline");
    case "rectangle": return closed("square", options.rectangleStyle);
    case "circle": return closed("circle", "outline");
    case "marker": return { tool: "marker", start, end };
    case "pixelate": return { tool: "pixelate", start, end, style: options.redact };
    case "invert": return { tool: "invert", start, end };
    case "circle_count": return { tool: "circle_counter", center: start, pointer: end, count: nextCounter };
    default: return null;
  }
}

/** The drag's end point moved to `point` (`draw_to`). */
export function dragTo(shape: Shape, point: Point): Shape {
  switch (shape.tool) {
    case "pencil": {
      const last = shape.points[shape.points.length - 1];
      return last && last.x === point.x && last.y === point.y
        ? shape
        : { ...shape, points: [...shape.points, point] };
    }
    case "circle_counter": return { ...shape, pointer: point };
    case "text": return shape;
    default: return { ...shape, end: point };
  }
}

// --- Shortcuts -----------------------------------------------------------------

export interface Shortcut {
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
}

const KEY_NAMES: Record<string, string> = {
  esc: "escape",
  escape: "escape",
  return: "enter",
  enter: "enter",
  del: "delete",
  delete: "delete",
  backspace: "backspace",
  ins: "insert",
  insert: "insert",
  tab: "tab",
  space: "space",
  left: "left",
  right: "right",
  up: "up",
  down: "down",
  home: "home",
  end: "end",
  pgup: "pageup",
  pageup: "pageup",
  pgdown: "pagedown",
  pagedown: "pagedown",
};

/** One chord as the settings store it (`Ctrl+Shift+c`, `Return`);
 * `null` for an empty value or a sequence of chords (`Shortcut::parse`). */
export function parseShortcut(text: string): Shortcut | null {
  const trimmed = text.trim();
  if (!trimmed || trimmed.includes(", ")) return null;
  const shortcut: Shortcut = { key: "", ctrl: false, shift: false, alt: false, meta: false };
  let rest = trimmed;
  for (;;) {
    const plus = rest.indexOf("+");
    // A trailing '+' is the plus key itself
    if (plus <= 0 || plus === rest.length - 1) break;
    const name = rest.slice(0, plus).toLowerCase();
    if (name === "ctrl") shortcut.ctrl = true;
    else if (name === "shift") shortcut.shift = true;
    else if (name === "alt") shortcut.alt = true;
    else if (name === "meta") shortcut.meta = true;
    else return null;
    rest = rest.slice(plus + 1);
  }
  const lower = rest.toLowerCase();
  if ([...rest].length === 1) shortcut.key = lower;
  else if (/^f([1-9]|1[0-9]|2[0-4])$/.test(lower)) shortcut.key = lower;
  else if (KEY_NAMES[lower]) shortcut.key = KEY_NAMES[lower];
  else return null;
  return shortcut;
}

export interface KeyLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** The key of a keyboard event as shortcuts name it: letters and digits
 * by their physical key, so Shift does not change them. */
export function eventKey(event: KeyLike): string {
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^Numpad[0-9]$/.test(event.code)) return event.code.slice(6);
  switch (event.key) {
    case "Escape": return "escape";
    case "Enter": return "enter";
    case "Delete": return "delete";
    case "Backspace": return "backspace";
    case "Insert": return "insert";
    case "Tab": return "tab";
    case " ": return "space";
    case "ArrowLeft": return "left";
    case "ArrowRight": return "right";
    case "ArrowUp": return "up";
    case "ArrowDown": return "down";
    case "Home": return "home";
    case "End": return "end";
    case "PageUp": return "pageup";
    case "PageDown": return "pagedown";
    default: return /^F([1-9]|1[0-9]|2[0-4])$/.test(event.key) ? event.key.toLowerCase() : event.key.toLowerCase();
  }
}

export function matchesShortcut(shortcut: Shortcut, event: KeyLike): boolean {
  return (
    shortcut.key === eventKey(event) &&
    shortcut.ctrl === event.ctrlKey &&
    shortcut.alt === event.altKey &&
    shortcut.meta === event.metaKey &&
    // `+` and `=` need Shift on most layouts; their shortcuts do not say so
    (shortcut.shift === event.shiftKey || ["+", "="].includes(shortcut.key))
  );
}

/** The shortcut as a tooltip shows it: `Ctrl+S`, `Enter`. */
export function shortcutLabel(shortcut: Shortcut): string {
  const parts: string[] = [];
  if (shortcut.ctrl) parts.push("Ctrl");
  if (shortcut.alt) parts.push("Alt");
  if (shortcut.shift) parts.push("Shift");
  if (shortcut.meta) parts.push("Win");
  const names: Record<string, string> = { escape: "Esc", enter: "Enter", delete: "Del", backspace: "Backspace", space: "Space" };
  parts.push(names[shortcut.key] ?? shortcut.key.toUpperCase());
  return parts.join("+");
}
