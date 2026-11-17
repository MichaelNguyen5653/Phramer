import { describe, expect, it } from "vitest";
import fixture from "../../tests/fixtures/settings-default.json";
import { insertToken, patternProblem, TOKENS } from "./pattern";
import { SECTIONS, SETTINGS } from "./schema";
import { boundedEditDistance, matches } from "./search";
import { canonical, heldModifiers, keycaps, parts, problem, sequenceFromEvent, type KeyLike } from "./shortcut";
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
import type { Snapshot } from "./types";
import { normalizeHex, paletteProblem, parseWhole } from "./validate";

const defaults = fixture as unknown as Snapshot;

function key(code: string, mods: Partial<KeyLike> = {}): KeyLike {
  return { key: "", code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods };
}

describe("settings state", () => {
  it("covers every setting the daemon sends, and nothing it does not", () => {
    const shown = SETTINGS.map((def) => def.key).filter((name) => name in defaults.values);
    // The Print Screen question's "Don't ask again" (Shortcuts section)
    shown.push("ignorePrntScrForcesSnipping");
    expect(new Set(shown)).toEqual(new Set(Object.keys(defaults.values)));
    for (const def of SETTINGS) {
      expect(SECTIONS.some((section) => section.id === def.section)).toBe(true);
    }
    // Every section has something in it
    for (const section of SECTIONS) expect(settingsIn(section.id).length).toBeGreaterThan(0);
  });

  it("knows what differs from the default", () => {
    expect(isDefault(defaults, "showHelp")).toBe(true);
    const changed = withValue(defaults, "showHelp", false);
    expect(isDefault(changed, "showHelp")).toBe(false);
    expect(defaults.values.showHelp).toBe(true);
    // Lists compare by content
    expect(isDefault(withValue(defaults, "buttons", [...(defaults.values.buttons as number[])]), "buttons")).toBe(true);
    // Print Screen's default is Windows keeping it
    expect(isDefault(defaults, "printScreen")).toBe(true);
    expect(isDefault({ ...defaults, snipping_claims_print_screen: false }, "printScreen")).toBe(false);
  });

  it("shows a shortcut before the daemon answers", () => {
    const next = withShortcut(defaults, "TYPE_PENCIL", "Q");
    expect(next.shortcuts.find((row) => row.name === "TYPE_PENCIL")?.value).toBe("Q");
    expect(defaults.shortcuts.find((row) => row.name === "TYPE_PENCIL")?.value).toBe("p");
  });

  it("names the rows a new shortcut took its key from", () => {
    const after = withShortcut(withShortcut(defaults, "TYPE_PENCIL", "g"), "TYPE_MOVE_OBJECT", "");
    expect(displaced(defaults, after, "TYPE_PENCIL").map((row) => row.name)).toEqual(["TYPE_MOVE_OBJECT"]);
    // Clearing one's own key displaces nothing
    expect(displaced(defaults, withShortcut(defaults, "TYPE_UNDO", ""), "TYPE_UNDO")).toEqual([]);
  });

  it("reports hotkeys another app holds", () => {
    const capture = defaults.shortcuts.find((row) => row.name === "TAKE_SCREENSHOT");
    if (!capture) throw new Error("no capture row");
    expect(hotkeyProblem(defaults, capture)).toBeNull();
    const taken = { ...defaults, hotkey_failures: ["Shift+Meta+X"] };
    expect(hotkeyProblem(taken, capture)).toMatch(/Another app/);
    expect(printScreenProblem({ ...defaults, snipping_claims_print_screen: false, hotkey_failures: ["Print"] })).toMatch(
      /Windows has not let go/,
    );
    expect(printScreenProblem({ ...defaults, hotkey_failures: ["Print"] })).toBeNull();
  });
});

describe("search", () => {
  it("tolerates typos, prefixes and run-together words, as 15.0.0 did", () => {
    expect(matches("tray", "Show the tray icon")).toBe(true);
    expect(matches("trayicn", "Show the tray icon")).toBe(true);
    expect(matches("magnifer", "Show the magnifier")).toBe(true);
    expect(matches("notif", "Show desktop notifications")).toBe(true);
    expect(matches("tray magnifier", "Show the tray icon")).toBe(false);
    expect(matches("", "anything")).toBe(true);
    expect(matches("zz", "Show the tray icon")).toBe(false);
    expect(boundedEditDistance("kitten", "sitting", 3)).toBe(3);
    expect(boundedEditDistance("a", "abcdef", 2)).toBe(3);
  });

  it("finds settings by keyword and shortcuts by key", () => {
    const found = search(defaults, "jpg");
    expect(found.settings.map((def) => def.key)).toEqual(expect.arrayContaining(["saveAsFileExtension", "jpegQuality"]));
    const pencil = search(defaults, "pencil");
    expect(pencil.shortcuts.some((row) => row.name === "TYPE_PENCIL")).toBe(true);
    expect(search(defaults, "printscreen").settings.map((def) => def.key)).toContain("printScreen");
    expect(search(defaults, "   ")).toEqual({ settings: [], shortcuts: [] });
  });
});

describe("validation", () => {
  it("takes whole numbers in range only", () => {
    expect(parseWhole("42", 0, 100)).toEqual({ value: 42 });
    expect(parseWhole(" 7 ", 1, 999)).toEqual({ value: 7 });
    expect(parseWhole("101", 0, 100)).toEqual({ error: "Enter 0 to 100" });
    expect(parseWhole("4.5", 0, 100)).toEqual({ error: "Enter a whole number" });
    expect(parseWhole("", 0, 100)).toEqual({ error: "Enter a whole number" });
  });

  it("normalizes colours", () => {
    expect(normalizeHex("#ABCDEF")).toBe("#abcdef");
    expect(normalizeHex("f80")).toBe("#ff8800");
    expect(normalizeHex("red")).toBeNull();
    expect(normalizeHex("#12345")).toBeNull();
  });

  it("holds the palette to 3..17 entries", () => {
    expect(paletteProblem([null, "#000000", "#ffffff"])).toBeNull();
    expect(paletteProblem([null, "#000000"])).toMatch(/at least 3/);
    expect(paletteProblem(Array(18).fill("#000000"))).toMatch(/at most 17/);
  });
});

describe("file name pattern", () => {
  it("inserts a token over the selection", () => {
    expect(insertToken("shot_", 5, 5, "%Y")).toEqual({ text: "shot_%Y", caret: 7 });
    expect(insertToken("a-XX-b", 2, 4, "%d")).toEqual({ text: "a-%d-b", caret: 4 });
    expect(insertToken("ab", 9, 9, "%H")).toEqual({ text: "ab%H", caret: 4 });
  });

  it("refuses what Windows refuses in a name, but not / and :", () => {
    expect(patternProblem("%F_%H-%M")).toBeNull();
    expect(patternProblem("%d/%m %H:%M")).toBeNull();
    expect(patternProblem("  ")).toMatch(/empty/);
    expect(patternProblem("a*b?c")).toBe("Windows does not allow * ? in a file name");
  });

  it("offers 15.0.0's Windows helpers", () => {
    expect(TOKENS).toHaveLength(15);
    expect(TOKENS.map(([, token]) => token)).toContain("%F");
  });
});

describe("shortcut recording", () => {
  it("writes Qt's portable text with Qt's modifier order", () => {
    expect(sequenceFromEvent(key("KeyX", { shiftKey: true, metaKey: true }))).toBe("Meta+Shift+X");
    expect(sequenceFromEvent(key("KeyS", { ctrlKey: true, altKey: true, shiftKey: true }))).toBe("Ctrl+Alt+Shift+S");
    expect(sequenceFromEvent(key("PrintScreen"))).toBe("Print");
    expect(sequenceFromEvent(key("Delete"))).toBe("Del");
    expect(sequenceFromEvent(key("Enter", { ctrlKey: true }))).toBe("Ctrl+Return");
    expect(sequenceFromEvent(key("ArrowLeft", { shiftKey: true }))).toBe("Shift+Left");
    expect(sequenceFromEvent(key("F12"))).toBe("F12");
    expect(sequenceFromEvent(key("Digit1"))).toBe("1");
    expect(sequenceFromEvent(key("Numpad5"))).toBe("Num+5");
    expect(sequenceFromEvent(key("NumpadAdd", { ctrlKey: true }))).toBe("Ctrl+Num++");
    expect(sequenceFromEvent(key("Comma", { ctrlKey: true }))).toBe("Ctrl+,");
  });

  it("waits while only modifiers are down", () => {
    expect(sequenceFromEvent(key("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(sequenceFromEvent(key("MetaLeft", { metaKey: true }))).toBeNull();
    expect(sequenceFromEvent(key("Unidentified"))).toBeNull();
    expect(heldModifiers({ ctrlKey: true, altKey: false, shiftKey: true, metaKey: false })).toEqual(["Ctrl", "Shift"]);
  });

  it("parses stored sequences into keycaps", () => {
    expect(parts("Meta+Shift+x")).toEqual(["Meta", "Shift", "x"]);
    expect(parts("Ctrl++")).toEqual(["Ctrl", "+"]);
    expect(parts("Ctrl+K, Ctrl+C")).toEqual(["Ctrl", "K"]);
    expect(parts("")).toEqual([]);
    expect(keycaps("Meta+Shift+x")).toEqual(["Win", "Shift", "X"]);
    expect(keycaps("Print")).toEqual(["Print Screen"]);
    expect(keycaps("Ctrl+Return")).toEqual(["Ctrl", "Enter"]);
    expect(keycaps("Del")).toEqual(["Delete"]);
  });

  it("compares keys whatever the order or case", () => {
    expect(canonical("Meta+Shift+x")).toBe(canonical("Shift+Meta+X"));
    expect(canonical("Ctrl+x")).not.toBe(canonical("Ctrl+Shift+x"));
    expect(canonical("")).toBe("");
  });

  it("refuses reserved keys and bare keys as global hotkeys", () => {
    expect(problem("Esc", false)).toMatch(/reserved/);
    expect(problem("Backspace", false)).toMatch(/reserved/);
    expect(problem("Ctrl+Backspace", false)).toBeNull();
    expect(problem("A", false)).toBeNull();
    expect(problem("A", true)).toMatch(/Add Ctrl/);
    expect(problem("Print", true)).toBeNull();
    expect(problem("F9", true)).toBeNull();
    expect(problem("Meta+Shift+X", true)).toBeNull();
    expect(problem("", true)).toBeNull();
  });
});
