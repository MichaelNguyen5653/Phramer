// Checks on what is typed, before anything is written. The daemon checks
// again; these give the answer at once.

/** A whole number in `min..max` from what was typed, or why not. */
export function parseWhole(text: string, min: number, max: number): { value: number } | { error: string } {
  const trimmed = text.trim();
  if (!/^-?\d+$/.test(trimmed)) return { error: "Enter a whole number" };
  const value = Number(trimmed);
  if (value < min || value > max) return { error: `Enter ${min} to ${max}` };
  return { value };
}

/** `#rrggbb` (lower case) from `#rgb`, `rrggbb` or `#RRGGBB`, or `null`. */
export function normalizeHex(text: string): string | null {
  const trimmed = text.trim().replace(/^#/, "");
  if (/^[0-9a-f]{3}$/i.test(trimmed)) {
    return `#${[...trimmed].map((c) => c + c).join("")}`.toLowerCase();
  }
  return /^[0-9a-f]{6}$/i.test(trimmed) ? `#${trimmed.toLowerCase()}` : null;
}

/** The palette (`userColors`) holds 3 to 17 entries, the picker included. */
export const PALETTE_MIN = 3;
export const PALETTE_MAX = 17;

export function paletteProblem(palette: readonly (string | null)[]): string | null {
  if (palette.length < PALETTE_MIN) return `The palette needs at least ${PALETTE_MIN} entries`;
  if (palette.length > PALETTE_MAX) return `The palette holds at most ${PALETTE_MAX} entries`;
  return null;
}
