// The whole project's activity network: every activity the grid
// shows, in columns by the longest chain of links from the open starts, so
// every link runs left to right. Pure and deterministic: the same rows,
// links and activities always give the same picture.
//
// Every column shares one grid of rows, so the gaps between rows are
// channels no box crosses: a link that skips columns runs out into the
// gutter after its predecessor, up or down to the channel above its
// successor's row, along it, and down the gutter before the successor.

import type { Links, Rows } from "../api";
import { BOX, type NetEdge, type NetNode, linksBy } from "./layout";

/** The most activities the whole-project network draws. */
export const PROJECT_LIMIT = 3000;

export interface ProjectNetwork {
  nodes: NetNode[];
  edges: NetEdge[];
  width: number;
  height: number;
  /** How many columns: the longest chain of links among them, plus one. */
  columns: number;
}

/**
 * The network of `members` (indexes into `rows`, such as the grid's lines),
 * each activity once. A node's `level` is its column: 0 for an activity with
 * no predecessor among them, else one more than its furthest predecessor's.
 */
export function projectNetworkOf(rows: Rows, links: Links, members: readonly number[]): ProjectNetwork {
  const indexOf = new Map<number, number>();
  for (const i of members) {
    const id = rows.id[i];
    if (id !== undefined && !indexOf.has(id)) indexOf.set(id, i);
  }
  const { into } = linksBy(links);
  const predecessorsOf = (id: number) =>
    (into.get(id) ?? []).map((k) => links.predecessor[k] ?? -1).filter((p) => p !== id && indexOf.has(p));

  // Longest chain from an open start, without recursion, so a long chain
  // can't overflow the stack. A loop, which valid logic can't hold, is cut where
  // it's met.
  const level = new Map<number, number>();
  const visiting = new Set<number>();
  // By task_id, so where a loop is cut doesn't depend on the grid's order.
  for (const start of [...indexOf.keys()].sort((a, b) => a - b)) {
    if (level.has(start)) continue;
    const stack: number[] = [start];
    while (stack.length > 0) {
      const id = stack[stack.length - 1] ?? -1;
      if (level.has(id)) {
        stack.pop();
        continue;
      }
      visiting.add(id);
      let waiting = false;
      for (const p of predecessorsOf(id)) {
        if (!level.has(p) && !visiting.has(p)) {
          stack.push(p);
          waiting = true;
        }
      }
      if (waiting) continue;
      let deepest = -1;
      for (const p of predecessorsOf(id)) deepest = Math.max(deepest, level.get(p) ?? -1);
      level.set(id, deepest + 1);
      visiting.delete(id);
      stack.pop();
    }
  }

  // Each column by early start, then Activity ID, then task_id.
  const columns: number[][] = [];
  for (const [id, l] of level) (columns[l] ??= []).push(id);
  const key = (id: number) => {
    const i = indexOf.get(id) ?? -1;
    return { start: rows.early_start[i] ?? null, code: rows.code[i] ?? "" };
  };
  for (const column of columns) {
    column.sort((a, b) => {
      const x = key(a);
      const y = key(b);
      if (x.start !== y.start) {
        if (x.start === null) return 1;
        if (y.start === null) return -1;
        return x.start < y.start ? -1 : 1;
      }
      if (x.code !== y.code) return x.code < y.code ? -1 : 1;
      return a - b;
    });
  }

  const stepX = BOX.width + BOX.gapX;
  const stepY = BOX.height + BOX.gapY;
  // Room above the first row for its channel, as there is between rows.
  const top = BOX.margin + BOX.gapY;
  const nodes: NetNode[] = [];
  const at = new Map<number, NetNode>();
  columns.forEach((column, l) => {
    column.forEach((id, row) => {
      const node = { id, index: indexOf.get(id) ?? -1, level: l, x: BOX.margin + l * stepX, y: top + row * stepY };
      nodes.push(node);
      at.set(id, node);
    });
  });

  const edges: NetEdge[] = [];
  // Links sharing a gutter or a channel are spread a few pixels apart, so
  // each can be followed: the nth to use one is offset by its place.
  const used = new Map<string, number>();
  const spread = (key: string, room: number) => {
    const n = used.get(key) ?? 0;
    used.set(key, n + 1);
    const slots = Math.max(1, Math.floor(room / 3));
    return ((n % slots) - Math.floor(slots / 2)) * 3;
  };
  links.id.forEach((link, k) => {
    const from = at.get(links.predecessor[k] ?? -1);
    const to = at.get(links.successor[k] ?? -1);
    if (!from || !to || from === to) return;
    const fromY = from.y + BOX.height / 2;
    const toY = to.y + BOX.height / 2;
    const gutter = BOX.gapX - 16;
    const out = from.x + BOX.width + BOX.gapX / 2 + spread(`x${from.x}`, gutter);
    const back = to.x - BOX.gapX / 2 + spread(`x${to.x - BOX.width - BOX.gapX}`, gutter);
    let points: { x: number; y: number }[];
    let label: { x: number; y: number };
    if (to.x === from.x + stepX) {
      // The next column: across, turning in the gutter between if the rows differ.
      points =
        fromY === toY
          ? [
              { x: from.x + BOX.width, y: fromY },
              { x: to.x, y: toY },
            ]
          : [
              { x: from.x + BOX.width, y: fromY },
              { x: out, y: fromY },
              { x: out, y: toY },
              { x: to.x, y: toY },
            ];
      label = { x: (out + to.x) / 2, y: toY - 4 };
    } else if (to.x > from.x) {
      // Skipping columns: along the channel above the successor's row.
      const channel = to.y - BOX.gapY / 2 + spread(`y${to.y}`, BOX.gapY - 4);
      points = [
        { x: from.x + BOX.width, y: fromY },
        { x: out, y: fromY },
        { x: out, y: channel },
        { x: back, y: channel },
        { x: back, y: toY },
        { x: to.x, y: toY },
      ];
      label = { x: (back + to.x) / 2, y: toY - 4 };
    } else {
      // Only a loop runs backwards; from the bottom of one box to the top of the other.
      points = [
        { x: from.x + BOX.width / 2, y: from.y + BOX.height },
        { x: to.x + BOX.width / 2, y: to.y },
      ];
      label = { x: (from.x + to.x) / 2 + BOX.width / 2, y: (from.y + BOX.height + to.y) / 2 };
    }
    edges.push({
      link,
      from: from.id,
      to: to.id,
      type: links.type[k] ?? "FS",
      lagHours: links.lag_hours[k] ?? 0,
      driving: links.driving[k] ?? false,
      points,
      label,
    });
  });

  const tallest = Math.max(0, ...columns.map((c) => c.length));
  return {
    nodes,
    edges,
    width: BOX.margin * 2 + columns.length * BOX.width + Math.max(0, columns.length - 1) * BOX.gapX,
    height: top + BOX.margin + tallest * BOX.height + Math.max(0, tallest - 1) * BOX.gapY,
    columns: columns.length,
  };
}
