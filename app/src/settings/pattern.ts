// The file name pattern (`filenamePattern`): 15.0.0's helper buttons, and
// what Windows refuses in a file name. The preview itself comes from the
// daemon, which expands the pattern exactly as a save does.

/** 15.0.0's Windows list (`StrftimeChooserWidget`), date before time. */
export const TOKENS: readonly (readonly [string, string])[] = [
  ["Year (2000)", "%Y"],
  ["Year (00-99)", "%y"],
  ["Century (00-99)", "%C"],
  ["Month (01-12)", "%m"],
  ["Week (01-53)", "%V"],
  ["Week Day (1-7)", "%u"],
  ["Day (01-31)", "%d"],
  ["Day of Month (1-31)", "%e"],
  ["Day (001-366)", "%j"],
  ["Hour (00-23)", "%H"],
  ["Hour (01-12)", "%I"],
  ["Minute (00-59)", "%M"],
  ["Second (00-59)", "%S"],
  ["Full Date (%Y-%m-%d)", "%F"],
  ["Full Date (%d-%m-%Y)", "%d-%m-%Y"],
];

/** `token` typed over the selection; where the caret goes after it. */
export function insertToken(
  text: string,
  start: number,
  end: number,
  token: string,
): { text: string; caret: number } {
  const from = Math.max(0, Math.min(start, end, text.length));
  const to = Math.min(text.length, Math.max(start, end));
  return { text: text.slice(0, from) + token + text.slice(to), caret: from + token.length };
}

/** Why the pattern cannot name a file, or `null`. A slash and a colon are
 *  allowed: the save replaces them. */
export function patternProblem(pattern: string): string | null {
  if (pattern.trim() === "") return "The pattern cannot be empty";
  const refused = [...new Set(pattern.match(/[<>"|?*\\]/g) ?? [])];
  if (refused.length > 0) return `Windows does not allow ${refused.join(" ")} in a file name`;
  return null;
}
