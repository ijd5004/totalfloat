// One activity's predecessors and successors, from `GET /api/1/links`.

import type { Links } from "../api";

/** One of an activity's relationships, from that activity's side. */
export interface Logic {
  /** `TASKPRED.task_pred_id`. */
  link: number;
  /** The activity at the other end, by `task_id`. */
  other: number;
  /** `FS`, `SS`, `FF` or `SF`. */
  type: string;
  lagHours: number;
  /** Whether it drives the successor's dates. */
  driving: boolean;
}

/** Each activity's links, by place in the answer: where it's the successor, and where it's the predecessor. */
interface Index {
  before: Map<number, number[]>;
  after: Map<number, number[]>;
}

const indexes = new WeakMap<Links, Index>();

/** Indexes `links` once per answer, so each selection reads its own in constant time. */
function indexOf(links: Links): Index {
  let index = indexes.get(links);
  if (index) return index;
  index = { before: new Map(), after: new Map() };
  for (let k = 0; k < links.count; k++) {
    const add = (map: Map<number, number[]>, id: number | undefined) => {
      if (id === undefined) return;
      const list = map.get(id);
      if (list) list.push(k);
      else map.set(id, [k]);
    };
    add(index.before, links.successor[k]);
    add(index.after, links.predecessor[k]);
  }
  indexes.set(links, index);
  return index;
}

/**
 * Activity `id`'s predecessors and successors, driving links first, then in
 * the order the server lists them.
 */
export function logicOf(links: Links, id: number): { predecessors: Logic[]; successors: Logic[] } {
  const index = indexOf(links);
  const read = (k: number, other: number | undefined): Logic => ({
    link: links.id[k] ?? -1,
    other: other ?? -1,
    type: links.type[k] ?? "FS",
    lagHours: links.lag_hours[k] ?? 0,
    driving: links.driving[k] ?? false,
  });
  const drivingFirst = (a: Logic, b: Logic) => Number(b.driving) - Number(a.driving);
  return {
    predecessors: (index.before.get(id) ?? []).map((k) => read(k, links.predecessor[k])).sort(drivingFirst),
    successors: (index.after.get(id) ?? []).map((k) => read(k, links.successor[k])).sort(drivingFirst),
  };
}
