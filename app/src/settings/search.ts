// The search box: 15.0.0's FuzzyMatch. Every word of the query must be in
// the text, start one of its words, be a small typo away from one, or (for
// three letters or more) appear in order with gaps ("trayicn").

/** Levenshtein distance, given up once it is certain to pass `limit`. */
export function boundedEditDistance(a: string, b: string, limit: number): number {
  if (Math.abs(a.length - b.length) > limit) return limit + 1;
  let previous = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const current = [i];
    let rowBest = i;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      current[j] = Math.min(current[j - 1] + 1, previous[j] + 1, previous[j - 1] + cost);
      rowBest = Math.min(rowBest, current[j]);
    }
    if (rowBest > limit) return limit + 1;
    previous = current;
  }
  return previous[b.length];
}

/** How wrong a word may be and still count as the same word. */
function typoBudget(length: number): number {
  if (length <= 3) return 1;
  return length <= 6 ? 2 : 3;
}

function isSubsequence(needle: string, haystack: string): boolean {
  let index = 0;
  for (const c of haystack) {
    if (c === needle[index] && ++index === needle.length) return true;
  }
  return false;
}

export function matches(query: string, text: string): boolean {
  const haystack = text.toLowerCase();
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;
  const words = haystack.split(/[^a-z0-9]+/).filter(Boolean);
  return terms.every((term) => {
    if (haystack.includes(term)) return true;
    const budget = typoBudget(term.length);
    if (words.some((word) => word.startsWith(term) || boundedEditDistance(term, word, budget) <= budget)) {
      return true;
    }
    // Short terms would match nearly everything in order
    return term.length >= 3 && isSubsequence(term, haystack);
  });
}
