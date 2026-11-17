// App Launcher's search, as 15.0.0's: the typed text is a wildcard pattern
// (`*` any run, `?` any one character) that may match anywhere in an app's
// name, ignoring case. Pure.

export interface AppEntry {
  name: string;
  exe: string;
  /** Offered for image files (15.0.0's Graphics tab), else Utility. */
  graphics: boolean;
}

/** `*text*` as a case-insensitive expression; other characters literal. */
export function wildcard(text: string): RegExp {
  const body = [...text]
    .map((c) => (c === "*" ? ".*" : c === "?" ? "." : c.replace(/[\\^$.|+()[\]{}/-]/g, "\\$&")))
    .join("");
  return new RegExp(`^.*${body}.*$`, "is");
}

/**
 * The apps matching `query` in list order (Graphics first), each name once:
 * a name matched again by another program is dropped, as 15.0.0 did.
 */
export function filterApps(apps: readonly AppEntry[], query: string): AppEntry[] {
  const pattern = query === "" ? null : wildcard(query);
  const names = new Set<string>();
  const found: AppEntry[] = [];
  for (const app of apps) {
    if (pattern && !pattern.test(app.name)) continue;
    if (names.has(app.name)) continue;
    names.add(app.name);
    found.push(app);
  }
  return found;
}

/** The selection after `key` in a list of `count` rows. */
export function moveSelection(current: number, key: string, count: number, page = 8): number {
  if (count <= 0) return 0;
  const last = count - 1;
  switch (key) {
    case "ArrowDown":
      return current >= last ? 0 : current + 1;
    case "ArrowUp":
      return current <= 0 ? last : current - 1;
    case "PageDown":
      return Math.min(last, current + page);
    case "PageUp":
      return Math.max(0, current - page);
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return Math.min(Math.max(current, 0), last);
  }
}
