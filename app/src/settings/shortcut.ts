// Key sequences as the settings file spells them: Qt's portable text, with
// modifiers in Qt's order (Meta, Ctrl, Alt, Shift, Num) and Qt's key names,
// so what the recorder writes reads back in 14.x and 15.x.

export interface KeyLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const NAMED: Record<string, string> = {
  Space: "Space",
  Escape: "Esc",
  Tab: "Tab",
  Backspace: "Backspace",
  Enter: "Return",
  Insert: "Ins",
  Delete: "Del",
  Pause: "Pause",
  PrintScreen: "Print",
  Home: "Home",
  End: "End",
  ArrowLeft: "Left",
  ArrowUp: "Up",
  ArrowRight: "Right",
  ArrowDown: "Down",
  PageUp: "PgUp",
  PageDown: "PgDown",
  CapsLock: "CapsLock",
  NumLock: "NumLock",
  ScrollLock: "ScrollLock",
  ContextMenu: "Menu",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

const KEYPAD: Record<string, string> = {
  NumpadAdd: "+",
  NumpadSubtract: "-",
  NumpadMultiply: "*",
  NumpadDivide: "/",
  NumpadDecimal: ".",
  NumpadEqual: "=",
  NumpadEnter: "Enter",
};

const MODIFIER_CODES = /^(Control|Shift|Alt|Meta|OS)(Left|Right)?$/;

/** The key alone, Qt-named; `null` for a modifier or an unknown key. */
function keyName(code: string): { name: string; keypad: boolean } | null {
  if (MODIFIER_CODES.test(code)) return null;
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return { name: letter[1], keypad: false };
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return { name: digit[1], keypad: false };
  const fn = /^F([0-9]{1,2})$/.exec(code);
  if (fn && Number(fn[1]) >= 1 && Number(fn[1]) <= 24) return { name: `F${fn[1]}`, keypad: false };
  const pad = /^Numpad([0-9])$/.exec(code);
  if (pad) return { name: pad[1], keypad: true };
  if (code in KEYPAD) return { name: KEYPAD[code], keypad: true };
  if (code in NAMED) return { name: NAMED[code], keypad: false };
  return null;
}

/** The sequence a key press makes, or `null` while only modifiers are down. */
export function sequenceFromEvent(event: KeyLike): string | null {
  const key = keyName(event.code);
  if (!key) return null;
  const parts: string[] = [];
  if (event.metaKey) parts.push("Meta");
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (key.keypad) parts.push("Num");
  parts.push(key.name);
  return parts.join("+");
}

/** The modifiers held, for the recorder to show before the key comes. */
export function heldModifiers(event: Pick<KeyLike, "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string[] {
  const held: string[] = [];
  if (event.metaKey) held.push("Meta");
  if (event.ctrlKey) held.push("Ctrl");
  if (event.altKey) held.push("Alt");
  if (event.shiftKey) held.push("Shift");
  return held;
}

/** Splits a sequence's first chord into its parts; `Ctrl++` is Ctrl and +. */
export function parts(sequence: string): string[] {
  const text = sequence.trim();
  if (!text) return [];
  const chord = text.split(/,\s?(?=.)/)[0] ?? text;
  if (chord.endsWith("++")) return [...chord.slice(0, -2).split("+").filter(Boolean), "+"];
  if (chord === "+") return ["+"];
  return chord.split("+").filter(Boolean);
}

const LABELS: Record<string, string> = {
  meta: "Win",
  return: "Enter",
  enter: "Enter",
  print: "Print Screen",
  del: "Delete",
  ins: "Insert",
  pgup: "Page Up",
  pgdown: "Page Down",
  esc: "Esc",
  num: "Num",
};

/** Keycaps for a sequence: `Meta+Shift+x` shows as Win, Shift, X. */
export function keycaps(sequence: string): string[] {
  return parts(sequence).map((part) => {
    const label = LABELS[part.toLowerCase()];
    if (label) return label;
    return part.length === 1 ? part.toUpperCase() : part;
  });
}

/** The same keys whatever the order or case of the text. */
export function canonical(sequence: string): string {
  const all = parts(sequence).map((part) => part.toLowerCase());
  if (all.length === 0) return "";
  const key = all[all.length - 1];
  return [...all.slice(0, -1).sort(), key].join("+");
}

const MODIFIERS = new Set(["meta", "ctrl", "alt", "shift"]);

/** Why `sequence` cannot be used, or `null`. */
export function problem(sequence: string, global: boolean): string | null {
  const all = parts(sequence);
  if (all.length === 0) return null;
  const key = all[all.length - 1].toLowerCase();
  if (all.length === 1 && (key === "esc" || key === "backspace")) {
    return `${keycaps(sequence).join("+")} is reserved and cannot be a shortcut`;
  }
  if (global) {
    // A plain letter as a system-wide hotkey would take it from every app
    const modified = all.slice(0, -1).some((part) => MODIFIERS.has(part.toLowerCase()));
    const standalone = /^f([1-9]|1[0-9]|2[0-4])$/.test(key) || key === "print" || key === "pause";
    if (!modified && !standalone) return "Add Ctrl, Alt, Shift or Win: a single key would stop working in every other app";
  }
  return null;
}
