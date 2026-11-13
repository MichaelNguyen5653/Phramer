/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ICONS } from "./Icon";
import {
  ACTIONS,
  TOOLS,
  dragTo,
  eventKey,
  matchesShortcut,
  objectSize,
  parseShortcut,
  shapeBetween,
  shortcutLabel,
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
