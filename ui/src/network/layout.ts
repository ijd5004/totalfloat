// The activity network (PERT) view's layout: the selected activity in the
// middle, its predecessors in columns to the left and its successors to the
// right, as many levels out as asked. Pure and deterministic: the same rows,
// links and selection always give the same picture.

import type { Links, Rows } from "../api";
import { spanText } from "../grid/columns";
import { dateText } from "../dates";

/** A box's size and the gaps between boxes, in CSS pixels. */
export const BOX = { width: 168, height: 64, gapX: 56, gapY: 16, margin: 12 } as const;
/** The space between the lanes of links that skip a column. */
const LANE = 18;

export interface NetNode {
  /** `task_id`. */
  id: number;
  /** Its index in `rows`. */
  index: number;
  /** Links from the selected activity: negative for predecessors, positive for successors. */
  level: number;
  x: number;
  y: number;
}

export interface NetEdge {
  /** `task_pred_id`. */
  link: number;
  from: number;
  to: number;
  type: string;
  lagHours: number;
  driving: boolean;
  /** Where the line runs, from the predecessor's box to the successor's. */
  points: { x: number; y: number }[];
  /** Where its type and lag are written. */
  label: { x: number; y: number };
}

export interface Network {
  nodes: NetNode[];
  edges: NetEdge[];
  width: number;
  height: number;
  /** How many activities each way the levels reached, before any were left out. */
  predecessors: number;
  successors: number;
}

/** Each activity's links, by `task_id`, as indexes into `links`; built once per answer. */
const linkIndexes = new WeakMap<Links, { into: Map<number, number[]>; outOf: Map<number, number[]> }>();
export function linksBy(links: Links): { into: Map<number, number[]>; outOf: Map<number, number[]> } {
  let found = linkIndexes.get(links);
  if (!found) {
    const into = new Map<number, number[]>();
    const outOf = new Map<number, number[]>();
    links.id.forEach((_, k) => {
      for (const [by, id] of [
        [outOf, links.predecessor[k]],
        [into, links.successor[k]],
      ] as const) {
        if (id === undefined) continue;
        const list = by.get(id);
        if (list) list.push(k);
        else by.set(id, [k]);
      }
    });
    found = { into, outOf };
    linkIndexes.set(links, found);
  }
  return found;
}

/** Each activity's index in `rows`, by `task_id`; built once per answer. */
const rowIndexes = new WeakMap<Rows, Map<number, number>>();
export function rowIndex(rows: Rows): Map<number, number> {
  let found = rowIndexes.get(rows);
  if (!found) {
    found = new Map(rows.id.map((id, k) => [id, k]));
    rowIndexes.set(rows, found);
  }
  return found;
}

/**
 * Each activity's distance from `centre` by the longest chain of links among
 * `members` (which holds `centre`), so that every link between two of them
 * runs one way. `next` gives an activity's neighbours away from `centre`'s
 * side: its successors for predecessors, measured back from `centre`.
 */
function longest(members: ReadonlySet<number>, centre: number, toward: (id: number) => number[]): Map<number, number> {
  const length = new Map<number, number>([[centre, 0]]);
  const measure = (id: number): number => {
    const known = length.get(id);
    if (known !== undefined) return known;
    // A loop can't arise in valid logic; guard against one anyway.
    length.set(id, 0);
    let best = 0;
    for (const other of toward(id)) {
      if (members.has(other)) best = Math.max(best, measure(other) + 1);
    }
    length.set(id, best);
    return best;
  };
  for (const id of members) measure(id);
  return length;
}

/** Activities `depth` links out from `centre`, each at the fewest links it takes. */
function reach(centre: number, depth: number, next: (id: number) => number[]): Map<number, number> {
  const distance = new Map<number, number>([[centre, 0]]);
  let ring = [centre];
  for (let step = 1; step <= depth && ring.length > 0; step++) {
    const out: number[] = [];
    for (const id of ring) {
      for (const other of next(id)) {
        if (distance.has(other)) continue;
        distance.set(other, step);
        out.push(other);
      }
    }
    ring = out;
  }
  distance.delete(centre);
  return distance;
}

/** The network around `centre` (a `task_id`), `depth` levels each way. */
export function networkOf(rows: Rows, links: Links, centre: number, depth: number): Network {
  const index = rowIndex(rows);
  const { into, outOf } = linksBy(links);
  const known = (ids: number[]) => ids.filter((id) => index.has(id));
  const predecessorsOf = (id: number) => known((into.get(id) ?? []).map((k) => links.predecessor[k] ?? -1));
  const successorsOf = (id: number) => known((outOf.get(id) ?? []).map((k) => links.successor[k] ?? -1));
  // Who's shown: within `depth` links each way. Where: by the longest chain
  // to or from the selection among them, so every link runs left to right.
  const before = reach(centre, depth, predecessorsOf);
  const after = reach(centre, depth, successorsOf);
  const back = longest(new Set([centre, ...before.keys()]), centre, successorsOf);
  const ahead = longest(new Set([centre, ...after.keys()]), centre, predecessorsOf);
  const level = new Map<number, number>([[centre, 0]]);
  for (const id of before.keys()) level.set(id, -(back.get(id) ?? 1));
  for (const id of after.keys()) if (!level.has(id)) level.set(id, ahead.get(id) ?? 1);

  // Each column by early start, then Activity ID, then task_id.
  const key = (id: number) => {
    const k = index.get(id) ?? -1;
    return { start: rows.early_start[k] ?? null, code: rows.code[k] ?? "", id };
  };
  const columns = new Map<number, number[]>();
  for (const [id, l] of level) {
    const column = columns.get(l);
    if (column) column.push(id);
    else columns.set(l, [id]);
  }
  for (const column of columns.values()) {
    column.sort((a, b) => {
      const x = key(a);
      const y = key(b);
      if (x.start !== y.start) {
        if (x.start === null) return 1;
        if (y.start === null) return -1;
        return x.start < y.start ? -1 : 1;
      }
      if (x.code !== y.code) return x.code < y.code ? -1 : 1;
      return x.id - y.id;
    });
  }
  const levels = [...columns.keys()].sort((a, b) => a - b);
  const left = levels[0] ?? 0;
  const tallest = Math.max(1, ...[...columns.values()].map((c) => c.length));
  const nodes: NetNode[] = [];
  const at = new Map<number, NetNode>();
  for (const l of levels) {
    const column = columns.get(l) ?? [];
    // A shorter column is centred against the tallest.
    const offset = ((tallest - column.length) * (BOX.height + BOX.gapY)) / 2;
    column.forEach((id, row) => {
      const node = {
        id,
        index: index.get(id) ?? -1,
        level: l,
        x: BOX.margin + (l - left) * (BOX.width + BOX.gapX),
        y: BOX.margin + offset + row * (BOX.height + BOX.gapY),
      };
      nodes.push(node);
      at.set(id, node);
    });
  }

  const edges: NetEdge[] = [];
  // Below the tallest column, a lane for each link that skips a column, so it
  // runs round the boxes between rather than under them.
  const bottom = BOX.margin + tallest * BOX.height + (tallest - 1) * BOX.gapY;
  let lanes = 0;
  links.id.forEach((link, k) => {
    const from = at.get(links.predecessor[k] ?? -1);
    const to = at.get(links.successor[k] ?? -1);
    if (!from || !to) return;
    const step = BOX.width + BOX.gapX;
    let points: { x: number; y: number }[];
    if (to.x === from.x + step) {
      // The next column: straight across.
      points = [
        { x: from.x + BOX.width, y: from.y + BOX.height / 2 },
        { x: to.x, y: to.y + BOX.height / 2 },
      ];
    } else if (to.x > from.x) {
      // Out of the box's side into the gutter, down the gutter to its lane,
      // along, and up the gutter before the other box: gutters hold no boxes.
      lanes += 1;
      const lane = bottom + lanes * LANE;
      const nudge = ((lanes - 1) % 6) * 3;
      const out = from.x + BOX.width + BOX.gapX / 2 - 6 + nudge;
      const back = to.x - BOX.gapX / 2 + 6 - nudge;
      points = [
        { x: from.x + BOX.width, y: from.y + BOX.height / 2 },
        { x: out, y: from.y + BOX.height / 2 },
        { x: out, y: lane },
        { x: back, y: lane },
        { x: back, y: to.y + BOX.height / 2 },
        { x: to.x, y: to.y + BOX.height / 2 },
      ];
    } else {
      // In one column: from the bottom of one box to the top of the other.
      points = [
        { x: from.x + BOX.width / 2, y: from.y + BOX.height },
        { x: to.x + BOX.width / 2, y: to.y },
      ];
    }
    // Over the middle of the line, or of its run along a lane.
    const a = points[points.length === 6 ? 2 : 0] ?? { x: 0, y: 0 };
    const b = points[points.length === 6 ? 3 : 1] ?? a;
    edges.push({
      link,
      from: from.id,
      to: to.id,
      type: links.type[k] ?? "FS",
      lagHours: links.lag_hours[k] ?? 0,
      driving: links.driving[k] ?? false,
      points,
      label: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 - 4 },
    });
  });

  const width = BOX.margin * 2 + levels.length * BOX.width + Math.max(0, levels.length - 1) * BOX.gapX;
  const height = bottom + BOX.margin + (lanes > 0 ? lanes * LANE + BOX.gapY : 0);
  return { nodes, edges, width, height, predecessors: before.size, successors: after.size };
}

/** A box's lines: ID and name, then dates and float. */
export function box(rows: Rows, node: NetNode): { code: string; name: string; dates: string; float: string; critical: boolean } {
  const i = node.index;
  // The day only, in the format chosen: a box has room for no more.
  const day = (text: string | null | undefined) => dateText(text, false) || "—";
  // In the grid's units: days of the activity's calendar, or hours.
  const float = spanText(rows.total_float_hours[i], rows.hours_per_day?.[i]);
  return {
    code: rows.code[i] ?? "",
    name: rows.name[i] ?? "",
    dates: `${day(rows.early_start[i])} → ${day(rows.early_finish[i])}`,
    float: float === "" ? "" : `TF ${float}`,
    critical: rows.critical[i] ?? false,
  };
}
