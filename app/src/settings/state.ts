// What the page derives from a snapshot: which settings differ from their
// default, the optimistic value shown while a write is on its way, which
// rows the search finds, and what a shortcut change did to other rows.

import { matches } from "./search";
import { SETTINGS, searchText, type SectionId, type SettingDef } from "./schema";
import { canonical } from "./shortcut";
import type { ShortcutRow, Snapshot, Value } from "./types";

export function sameValue(a: Value | undefined, b: Value | undefined): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Whether `key` reads as its default. */
export function isDefault(snapshot: Snapshot, key: string): boolean {
  if (key === "printScreen") return snapshot.snipping_claims_print_screen;
  if (!(key in snapshot.defaults)) return true;
  return sameValue(snapshot.values[key], snapshot.defaults[key]);
}

/** The snapshot with `key` already showing `value`. */
export function withValue(snapshot: Snapshot, key: string, value: Value): Snapshot {
  return { ...snapshot, values: { ...snapshot.values, [key]: value } };
}

/** The snapshot with shortcut `name` already showing `sequence`. */
export function withShortcut(snapshot: Snapshot, name: string, sequence: string): Snapshot {
  return {
    ...snapshot,
    shortcuts: snapshot.shortcuts.map((row) => (row.name === name ? { ...row, value: sequence } : row)),
  };
}

export interface Found {
  settings: SettingDef[];
  shortcuts: ShortcutRow[];
}

/** What the search box finds: settings by label, help and keywords;
 *  shortcuts by description or key. */
export function search(snapshot: Snapshot, query: string): Found {
  const trimmed = query.trim();
  if (!trimmed) return { settings: [], shortcuts: [] };
  return {
    settings: SETTINGS.filter((def) => matches(trimmed, searchText(def))),
    shortcuts: snapshot.shortcuts.filter((row) => matches(trimmed, `${row.description} ${row.value} shortcut key`)),
  };
}

export function settingsIn(section: SectionId): SettingDef[] {
  return SETTINGS.filter((def) => def.section === section);
}

/** Rows that lost their key because `name` took it (a default gives way). */
export function displaced(before: Snapshot, after: Snapshot, name: string): ShortcutRow[] {
  return after.shortcuts.filter((row) => {
    if (!row.name || row.name === name || row.value) return false;
    const old = before.shortcuts.find((entry) => entry.name === row.name);
    return Boolean(old?.value);
  });
}

/** Why the hotkey `row` does not work now, if it does not. */
export function hotkeyProblem(snapshot: Snapshot, row: ShortcutRow): string | null {
  if (!row.global || !row.value) return null;
  const failed = snapshot.hotkey_failures.some((text) => canonical(text) === canonical(row.value));
  return failed ? "Another app is using this key, so it does nothing until that app lets go of it" : null;
}

/** Print Screen is Phramer's, but Windows has not let go of it yet. */
export function printScreenProblem(snapshot: Snapshot): string | null {
  if (snapshot.snipping_claims_print_screen) return null;
  const failed = snapshot.hotkey_failures.some((text) => canonical(text) === "print");
  return failed ? "Windows has not let go of the key yet. Restart Phramer, or sign out and back in." : null;
}
