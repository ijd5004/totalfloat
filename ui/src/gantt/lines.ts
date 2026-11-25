// Logic lines between Gantt bars: which ends a relationship joins, the
// orthogonal route between them, and where a line leaves the screen.

import type { Links } from "../api";

export type LinkType = "FS" | "SS" | "FF" | "SF";

/** A bar on screen: its left and right ends, and the middle of its row. */
export interface Ends {
  left: number;
  right: number;
  y: number;
}

export type Point = readonly [number, number];

/** Which end of each bar a relationship joins: FS leaves the finish and enters the start, and so on. */
export function joins(type: LinkType): { from: "start" | "finish"; to: "start" | "finish" } {
  switch (type) {
    case "FS":
      return { from: "finish", to: "start" };
    case "SS":
      return { from: "start", to: "start" };
    case "FF":
      return { from: "finish", to: "finish" };
    case "SF":
      return { from: "start", to: "finish" };
  }
}

/** How far a line runs out from a bar end before it turns. */
export const STUB = 6;

/**
 * The route from the predecessor's end to the successor's: out of its end,
 * across, and into the successor's end from outside the bar, as an arrow
 * meets a start from the left and a finish from the right. Where the
 * successor's end is behind where the line turns, it doglegs along the row
 * boundary below the predecessor.
 */
export function route(type: LinkType, from: Ends, to: Ends, rowHeight: number): Point[] {
  const { from: leaves, to: enters } = joins(type);
  const a = leaves === "finish" ? from.right : from.left;
  const b = enters === "start" ? to.left : to.right;
  // Out to the right of a finish, to the left of a start.
  const out = leaves === "finish" ? 1 : -1;
  // A start is entered moving right, a finish moving left.
  const into = enters === "start" ? 1 : -1;
  const turn = a + out * STUB;
  const direct = into === 1 ? turn <= b - STUB / 2 : turn >= b + STUB / 2;
  if (direct) {
    return [
      [a, from.y],
      [turn, from.y],
      [turn, to.y],
      [b, to.y],
    ];
  }
  const between = from.y + Math.sign(to.y - from.y || 1) * (rowHeight / 2);
  const approach = b - into * STUB;
  return [
    [a, from.y],
    [turn, from.y],
    [turn, between],
    [approach, between],
    [approach, to.y],
    [b, to.y],
  ];
}

export type Edge = "top" | "bottom" | "left" | "right";

/**
 * Where a route crosses an edge of the screen: where an arrow to the end
 * beyond goes. A route with both ends off screen crosses twice.
 */
export function edgesOf(
  points: readonly Point[],
  height: number,
  width = Number.POSITIVE_INFINITY,
): { x: number; y: number; edge: Edge; toward: "from" | "to" }[] {
  const out: { x: number; y: number; edge: Edge; toward: "from" | "to" }[] = [];
  for (let k = 1; k < points.length; k++) {
    const [x0, y0] = points[k - 1] ?? [0, 0];
    const [x1, y1] = points[k] ?? [0, 0];
    // From outside to on or inside the edge, or back: one end beyond it and
    // the other not. A route can run along an edge exactly. The part of the
    // route beyond the crossing leads to the end of the line on that side.
    const at = (t: number): [number, number] => [x0 + t * (x1 - x0), y0 + t * (y1 - y0)];
    const cross = (beyond0: boolean, beyond1: boolean, t: number, edge: Edge) => {
      if (beyond0 !== beyond1) out.push({ ...xy(at(t)), edge, toward: beyond1 ? "to" : "from" });
    };
    cross(y0 < 0, y1 < 0, (0 - y0) / (y1 - y0), "top");
    cross(y0 > height, y1 > height, (height - y0) / (y1 - y0), "bottom");
    cross(x0 < 0, x1 < 0, (0 - x0) / (x1 - x0), "left");
    cross(x0 > width, x1 > width, (width - x0) / (x1 - x0), "right");
  }
  // Only crossings on the screen itself lead anywhere.
  return out.filter((c) => c.x >= 0 && c.x <= width && c.y >= 0 && c.y <= height);
}

function xy([x, y]: [number, number]): { x: number; y: number } {
  return { x, y };
}

/** The distance from a point to a route, for hovering over a line. */
export function distanceTo(points: readonly Point[], x: number, y: number): number {
  let best = Number.POSITIVE_INFINITY;
  for (let k = 1; k < points.length; k++) {
    const [x0, y0] = points[k - 1] ?? [0, 0];
    const [x1, y1] = points[k] ?? [0, 0];
    const dx = x1 - x0;
    const dy = y1 - y0;
    const length = dx * dx + dy * dy;
    const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((x - x0) * dx + (y - y0) * dy) / length));
    best = Math.min(best, Math.hypot(x - (x0 + t * dx), y - (y0 + t * dy)));
  }
  return best;
}

/** Each activity's relationships, by `task_id`, as indexes into the links, built once per set of links. */
export function linksByActivity(links: Links): Map<number, number[]> {
  const known = cache.get(links);
  if (known) return known;
  const out = new Map<number, number[]>();
  const add = (id: number | undefined, k: number) => {
    if (id === undefined) return;
    const list = out.get(id);
    if (list) list.push(k);
    else out.set(id, [k]);
  };
  for (let k = 0; k < links.count; k++) {
    add(links.predecessor[k], k);
    add(links.successor[k], k);
  }
  cache.set(links, out);
  return out;
}

const cache = new WeakMap<Links, Map<number, number[]>>();

const lines = new WeakMap<Int32Array, Map<number, number>>();

/** Where each activity is in an arrangement, by `task_id`, built once per arrangement. */
export function lineIndex(ids: readonly number[], arranged: Int32Array): Map<number, number> {
  const known = lines.get(arranged);
  if (known) return known;
  const map = new Map<number, number>();
  arranged.forEach((i, line) => {
    if (i >= 0) map.set(ids[i] ?? -1, line);
  });
  lines.set(arranged, map);
  return map;
}
