// Drawing a relationship on the Gantt: which bar end the pointer is on, and
// the type the two ends make. The edge dragged from and the edge dropped on
// say which ends the link ties, so there's no modifier key to learn.

import type { Drawn } from "./draw";

/** A bar's start or finish. */
export type End = "start" | "finish";

/** One end of a bar on screen, by `task_id`. */
export interface Edge {
  id: number;
  end: End;
  /** Where the end is, in the chart body's pixels. */
  x: number;
  y: number;
}

/** How near, in pixels, the pointer must be to a bar's end to take it. */
export const EDGE_REACH = 6;

/** The relationship type the two ends make: finish to start is FS, and so on. */
export function typeOf(from: End, to: End): "FS" | "SS" | "FF" | "SF" {
  if (from === "finish") return to === "start" ? "FS" : "FF";
  return to === "start" ? "SS" : "SF";
}

/** The words for a type, as the tooltip names it. */
export const TYPE_WORDS: Record<"FS" | "SS" | "FF" | "SF", string> = {
  FS: "finish to start",
  SS: "start to start",
  FF: "finish to finish",
  SF: "start to finish",
};

/**
 * The bar end at a point, if the pointer is within `reach` of one: a bar's
 * left edge is its start and its right edge its finish. A milestone's
 * diamond is both, split down its middle.
 */
export function edgeAt(bars: Drawn["bars"], x: number, y: number, reach = EDGE_REACH): Edge | null {
  let best: Edge | null = null;
  let nearest = Number.POSITIVE_INFINITY;
  for (const bar of bars) {
    if (y < bar.top || y >= bar.bottom || x < bar.left - reach || x > bar.right + reach) continue;
    // The nearer end; on a bar too short to tell its ends apart, the half the pointer is in.
    const end: End = x - bar.left <= bar.right - x ? "start" : "finish";
    const at = end === "start" ? bar.left : bar.right;
    const off = Math.abs(x - at);
    const short = bar.right - bar.left <= 2 * reach;
    if ((short || off <= reach) && off < nearest) {
      nearest = off;
      best = { id: bar.id, end, x: at, y: (bar.top + bar.bottom) / 2 };
    }
  }
  return best;
}

/**
 * The end of another bar a drag would drop on: one the pointer is within
 * reach of, or, over the body of a bar, its nearer end, so a drop anywhere on
 * a bar links to it. Never the bar dragged from.
 */
export function dropAt(bars: Drawn["bars"], x: number, y: number, from: number): Edge | null {
  const others = bars.filter((bar) => bar.id !== from);
  const edge = edgeAt(others, x, y);
  if (edge) return edge;
  const bar = others.find((b) => y >= b.top && y < b.bottom && x >= b.left && x <= b.right);
  if (!bar) return null;
  const end: End = x - bar.left <= bar.right - x ? "start" : "finish";
  return { id: bar.id, end, x: end === "start" ? bar.left : bar.right, y: (bar.top + bar.bottom) / 2 };
}
