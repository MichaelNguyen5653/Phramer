/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { followShortcuts } from "./backend";
import { ICONS } from "./Icon";
import type { EditorSettings } from "./state";
import {
  ACTIONS,
  TOOLS,
  dragTo,
  eventKey,
  matchesShortcut,
  objectSize,
  parseShortcut,
  shapeBetween,
  shortcutHint,
  shortcutLabel,
  typeSizeDigit,
  type ToolOptions,
} from "./tools";

const overlayTools = readFileSync(new URL("../../../crates/phramer-overlay/src/state/tools.rs", import.meta.url), "utf8");
const overlayIcons = readFileSync(new URL("../../../crates/phramer-overlay/src/icons.rs", import.meta.url), "utf8");

/** `button_info`'s (name, description, icon, shortcut) per `ButtonType`. */
function overlayButtons(): Map<string, string[]> {
  const buttons = new Map<string, string[]>();
  const pattern = /ButtonType::(\w+) => \(\s*((?:"[^"]*",?\s*){4})\)/g;
  for (const match of overlayTools.matchAll(pattern)) {
    buttons.set(match[1], [...match[2].matchAll(/"([^"]*)"/g)].map((m) => m[1]));
  }
  return buttons;
}

/** `TOOLBAR_ORDER`. */
function overlayOrder(): string[] {
  const body = /TOOLBAR_ORDER: \[ButtonType; \d+\] = \[([\s\S]*?)\];/.exec(overlayTools)?.[1] ?? "";
  return [...body.matchAll(/ButtonType::(\w+)/g)].map((m) => m[1]);
}

const BUTTON_OF: Record<string, string> = {
  pencil: "Pencil",
  line: "Line",
  arrow: "Arrow",
  shape: "Shape",
  selection: "Selection",
  rectangle: "Rectangle",
  circle: "Circle",
  marker: "Marker",
  text: "Text",
  circle_count: "CircleCount",
  pixelate: "Pixelate",
  invert: "Invert",
  move_object: "MoveObject",
};

const options: ToolOptions = {
  arrowHead: "curved",
  reverseArrow: true,
  shapeKind: "circle",
  shapeStyle: "highlight",
  rectangleStyle: "filled",
  highlightOutline: 2,
  redact: "insecure",
  textAlign: "left",
  underline: false,
  strikeout: false,
  fontFamily: "",
  bold: false,
  italic: false,
};

describe("tools", () => {
  it("are the overlay's, in its order, with its icons, names and shortcuts", () => {
    const buttons = overlayButtons();
    const order = overlayOrder().filter((button) => Object.values(BUTTON_OF).includes(button));
    expect(TOOLS.map((tool) => BUTTON_OF[tool.id])).toEqual(order);
    for (const tool of TOOLS) {
      const [name, description, icon, shortcut] = buttons.get(BUTTON_OF[tool.id]) ?? [];
      expect({ name: tool.name, description: tool.description, icon: tool.icon, shortcut: tool.shortcut }).toEqual({
        name,
        description,
        icon,
        shortcut,
      });
    }
  });

  it("draw every icon by its Lucide name, the overlay's included", () => {
    for (const entry of [...TOOLS, ...ACTIONS]) expect(ICONS[entry.icon], entry.icon).toBeDefined();
    const vendored = [...overlayIcons.matchAll(/\(\s*"([a-z0-9-]+)",\s*include_str!/g)].map((m) => m[1]);
    expect(vendored.length).toBeGreaterThan(20);
    for (const name of vendored) expect(ICONS[name], name).toBeDefined();
  });

  it("scale sizes as the overlay does", () => {
    expect(objectSize("line", 3, 1)).toBe(3);
    expect(objectSize("line", 3, 1.5)).toBe(5);
    expect(objectSize("marker", 5, 1)).toBe(19);
    expect(objectSize("text", 8, 2)).toBe(24);
    expect(objectSize("circle_counter", 1, 2)).toBe(17);
    expect(objectSize("pixelate", 4, 3)).toBe(4);
  });

  it("draw the shapes the overlay draws", () => {
    const a = { x: 1, y: 2 };
    const b = { x: 30, y: 40 };
    expect(shapeBetween("shape", a, b, options, 1, 1)).toEqual({ tool: "ellipse", start: a, end: b, style: "highlight", outline_width: 2 });
    expect(shapeBetween("selection", a, b, options, 2, 1)).toEqual({ tool: "rectangle", start: a, end: b, style: "outline", outline_width: 4 });
    expect(shapeBetween("arrow", a, b, options, 1, 1)).toEqual({ tool: "arrow", start: a, end: b, head: "curved", reversed: true });
    expect(shapeBetween("pixelate", a, b, options, 1, 1)).toEqual({ tool: "pixelate", start: a, end: b, style: "insecure" });
    expect(shapeBetween("circle_count", a, a, options, 1, 7)).toEqual({ tool: "circle_counter", center: a, pointer: a, count: 7 });
    expect(shapeBetween("move_object", a, b, options, 1, 1)).toBeNull();
    const pencil = shapeBetween("pencil", a, a, options, 1, 1)!;
    expect(dragTo(dragTo(pencil, b), b)).toEqual({ tool: "pencil", points: [a, a, b] });
  });
});

describe("shortcuts", () => {
  const key = (code: string, k: string, mods: Partial<Record<"ctrlKey" | "shiftKey" | "altKey" | "metaKey", boolean>> = {}) => ({
    code,
    key: k,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...mods,
  });

  it("parse as the settings store them", () => {
    expect(parseShortcut("Ctrl+Shift+z")).toEqual({ key: "z", ctrl: true, shift: true, alt: false, meta: false });
    expect(parseShortcut("Return")?.key).toBe("enter");
    expect(parseShortcut("Del")?.key).toBe("delete");
    expect(parseShortcut("Ctrl++")?.key).toBe("+");
    expect(parseShortcut("")).toBeNull();
    expect(parseShortcut("Ctrl+K, Ctrl+C")).toBeNull();
    expect(parseShortcut("Hyper+x")).toBeNull();
  });

  it("match key presses by physical letter", () => {
    const undo = parseShortcut("Ctrl+Z")!;
    expect(matchesShortcut(undo, key("KeyZ", "z", { ctrlKey: true }))).toBe(true);
    expect(matchesShortcut(undo, key("KeyZ", "Z", { ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(matchesShortcut(parseShortcut("P")!, key("KeyP", "p"))).toBe(true);
    expect(matchesShortcut(parseShortcut("Ctrl+=")!, key("Equal", "+", { ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(eventKey(key("Delete", "Delete"))).toBe("delete");
    expect(shortcutLabel(parseShortcut("Ctrl+Shift+c")!)).toBe("Ctrl+Shift+C");
  });
});

describe("toolbar key hints", () => {
  const settings = {
    TYPE_PENCIL: "P",
    TYPE_UNDO: "Ctrl+Z",
    TYPE_REDO: "Ctrl+Shift+z",
    TYPE_COMMIT_CURRENT_TOOL: "Ctrl+Return",
    TYPE_DELETE_CURRENT_TOOL: "Del",
    TYPE_SHAPE: "",
    TYPE_PIN: "   ",
    TYPE_OCR: "Ctrl+K, Ctrl+O",
    TYPE_SAVE: "F5",
    TYPE_COPY: "Alt+PgDown",
  };

  it("show an assigned key in brackets, as the overlay's tooltips write it", () => {
    expect(shortcutHint("TYPE_PENCIL", settings)).toBe("(P)");
    expect(shortcutHint("TYPE_UNDO", settings)).toBe("(Ctrl+Z)");
    expect(shortcutHint("TYPE_REDO", settings)).toBe("(Ctrl+Shift+Z)");
    expect(shortcutHint("TYPE_COMMIT_CURRENT_TOOL", settings)).toBe("(Ctrl+Enter)");
    expect(shortcutHint("TYPE_DELETE_CURRENT_TOOL", settings)).toBe("(Del)");
    expect(shortcutHint("TYPE_SAVE", settings)).toBe("(F5)");
    expect(shortcutHint("TYPE_COPY", settings)).toBe("(Alt+PgDown)");
  });

  it("show nothing for a key that is not assigned or cannot be used", () => {
    expect(shortcutHint("TYPE_SHAPE", settings)).toBeNull();
    expect(shortcutHint("TYPE_PIN", settings)).toBeNull();
    expect(shortcutHint("TYPE_CIRCLECOUNT", settings)).toBeNull();
    // A two-chord sequence: the editor cannot press it either
    expect(shortcutHint("TYPE_OCR", settings)).toBeNull();
    // A button with no [Shortcuts] entry (Crop, Save As)
    expect(shortcutHint(undefined, settings)).toBeNull();
  });

  it("name keys as the overlay's Shortcut::label does", () => {
    const label = (text: string) => shortcutLabel(parseShortcut(text)!);
    expect(label("Ins")).toBe("Ins");
    expect(label("Esc")).toBe("Esc");
    expect(label("Ctrl+Left")).toBe("Ctrl+Left");
    expect(label("PgUp")).toBe("PgUp");
    expect(label("Meta+Shift+x")).toBe("Shift+Win+X");
  });

  it("follow the settings when they change", async () => {
    let shortcuts: Record<string, string> = { TYPE_PENCIL: "P", TYPE_SHAPE: "" };
    let changed: (() => void) | null = null;
    const source = {
      settings: async () => ({ shortcuts }) as EditorSettings,
      onSettingsChanged: async (handler: () => void) => {
        changed = handler;
        return () => {
          changed = null;
        };
      },
    };
    let seen: Record<string, string> | null = null;
    const stop = await followShortcuts(source, (next) => (seen = next));
    expect(seen).toBeNull();
    shortcuts = { TYPE_PENCIL: "", TYPE_SHAPE: "H" };
    changed!();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(shortcutHint("TYPE_PENCIL", seen!)).toBeNull();
    expect(shortcutHint("TYPE_SHAPE", seen!)).toBe("(H)");
    stop();
    expect(changed).toBeNull();
  });
});

describe("digit keys", () => {
  it("type a size digit by digit, as on the overlay", () => {
    let step = typeSizeDigit(null, 1);
    expect(step).toEqual({ size: 1, typed: 1 });
    step = typeSizeDigit(step.typed, 5);
    expect(step).toEqual({ size: 15, typed: 15 });
    // Past the largest size: clamped, and typing starts over
    step = typeSizeDigit(step.typed, 7);
    expect(step).toEqual({ size: 50, typed: null });
    expect(typeSizeDigit(step.typed, 3)).toEqual({ size: 3, typed: 3 });
    // 0 alone is below the smallest size
    expect(typeSizeDigit(null, 0)).toEqual({ size: 1, typed: null });
  });

  it("leave Z and Ctrl+' to Zoom mode and the grid, behind the configured keys", () => {
    expect(ACTIONS.find((action) => action.id === "zoom_mode")?.icon).toBe("search");
    expect(ACTIONS.find((action) => action.id === "grid")?.icon).toBe("grid-2x2");
    const grid = parseShortcut("Ctrl+'")!;
    expect(matchesShortcut(grid, { code: "Quote", key: "'", ctrlKey: true, shiftKey: false, altKey: false, metaKey: false })).toBe(true);
  });
});
