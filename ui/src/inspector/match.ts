// The Add predecessor and Add successor fields' matches: activities whose ID
// or name holds what's typed, best first.

import type { Rows } from "../api";

/** How many matches the list shows. */
export const MATCHES_SHOWN = 8;

/**
 * Up to `limit` activities, by place in `rows`, whose ID or name holds
 * `typed`, ignoring case: an exact ID first, then IDs that start with it,
 * IDs that hold it, names that start with it, and names that hold it, each
 * in the rows' order. `self` (a place in `rows`) is never listed.
 */
export function matches(rows: Rows, typed: string, self: number, limit = MATCHES_SHOWN): number[] {
  const wanted = typed.trim().toLowerCase();
  if (wanted === "") return [];
  const ranked: number[][] = [[], [], [], [], []];
  for (let i = 0; i < rows.count; i++) {
    if (i === self) continue;
    const code = (rows.code[i] ?? "").toLowerCase();
    const name = (rows.name[i] ?? "").toLowerCase();
    const rank =
      code === wanted
        ? 0
        : code.startsWith(wanted)
          ? 1
          : code.includes(wanted)
            ? 2
            : name.startsWith(wanted)
              ? 3
              : name.includes(wanted)
                ? 4
                : -1;
    if (rank < 0) continue;
    const bucket = ranked[rank];
    // The best buckets alone can fill the list; the rest needn't grow past it.
    if (bucket && bucket.length < limit) bucket.push(i);
    if (rank === 0 && ranked[0]?.length === limit) break;
  }
  return ranked.flat().slice(0, limit);
}

/** Where `typed` falls in `text`, ignoring case, for showing it bold: [before, match, after], or null. */
export function split(text: string, typed: string): [string, string, string] | null {
  const wanted = typed.trim().toLowerCase();
  const at = wanted ? text.toLowerCase().indexOf(wanted) : -1;
  if (at < 0) return null;
  return [text.slice(0, at), text.slice(at, at + wanted.length), text.slice(at + wanted.length)];
}
