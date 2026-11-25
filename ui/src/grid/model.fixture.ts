// Test data and work shared by `model.test.ts` and `model.budget.ts`: a
// synthetic schedule, and the grid's work on 50,000 activities that both
// time, so the unit test and the budget measure the same thing.

import type { Rows } from "../api";
import { DEFAULT_ARRANGEMENT, type Arrangement, type Grouping, type Model, arrange, filtered, sorted } from "./model";

const STATUSES = ["not-started", "in-progress", "completed"];

/** A synthetic schedule of `n` activities over a three-level WBS. */
export function synthetic(n: number): Rows {
  const pick = <T>(f: (i: number) => T) => Array.from({ length: n }, (_, i) => f(i));
  const day = (i: number) => `2026-${String((i % 12) + 1).padStart(2, "0")}-${String((i % 28) + 1).padStart(2, "0")} 08:00`;
  return {
    schema: "totalfloat.rows/1",
    file: "big.xer",
    count: n,
    id: pick((i) => i + 1),
    project: pick(() => 1),
    code: pick((i) => `A${(i * 7919) % n}`),
    name: pick((i) => `Activity ${i} ${i % 3 === 0 ? "pour concrete" : "install steel"}`),
    type: pick(() => "task"),
    status: pick((i) => STATUSES[i % 3] ?? "not-started"),
    critical: pick((i) => i % 5 === 0),
    calendar: pick((i) => (i % 2 ? "Five day" : "Seven day")),
    wbs_path: pick((i) => `PLANT.AREA${i % 10}.UNIT${i % 37}`),
    original_duration_hours: pick((i) => (i % 40) * 8),
    remaining_duration_hours: pick((i) => (i % 40) * 4),
    total_float_hours: pick((i) => (i % 9 === 0 ? null : (i % 101) - 50)),
    free_float_hours: pick(() => 0),
    early_start: pick(day),
    early_finish: pick(day),
    late_start: pick(day),
    late_finish: pick(day),
    actual_start: pick(() => null),
    actual_finish: pick(() => null),
    constraint: pick(() => null),
    constraint_date: pick(() => null),
    secondary_constraint: pick(() => null),
    secondary_constraint_date: pick(() => null),
  } as unknown as Rows;
}

/** The default arrangement with `change` applied. */
export const with_ = (change: Partial<Arrangement>): Arrangement => ({ ...DEFAULT_ARRANGEMENT, ...change });

/** How many activities the work at scale runs on. */
export const SCALE = 50_000;

/** Twelve phases, as the server's `code:Phase` column gives them; every seventh activity holds none. */
export const PHASE: Grouping = (() => {
  const values = new Map<number, string>();
  for (let i = 0; i < SCALE; i++) if (i % 7 !== 0) values.set(i + 1, `PH${(i % 12) + 1}`);
  return { field: "code:Phase", title: "Phase", values };
})();

/** A user field holding a value for every activity, from `at`. */
function field(name: string, at: (i: number) => number): Grouping {
  const values = new Map<number, string>();
  for (let i = 0; i < SCALE; i++) values.set(i + 1, `${name}${at(i)}`);
  return { field: `udf:${name}`, title: name, values };
}

/** 20,000 rooms, two or three activities in each. */
export const ROOM = field("Room", (i) => i % 20_000);
/** 20,000 tags, spread across the rooms. */
export const TAG = field("Tag", (i) => (i * 7) % 20_000);

const byCode = [{ column: "code" as const, descending: false }];

/** The grid's work on `SCALE` activities, one entry for each thing timed. */
export const AT_SCALE = {
  filter: (model: Model) => filtered(model, with_({ text: "pour", quick: new Set(["critical"]) })),
  sortTwoKeys: (model: Model) =>
    sorted(model, filtered(model, with_({})), [
      { column: "float", descending: false },
      { column: "code", descending: true },
    ]),
  wbs: (model: Model) => arrange(model, with_({ sort: byCode })),
  phase: (model: Model) => arrange(model, with_({ by: PHASE, sort: byCode })),
  wbsThenPhase: (model: Model) => arrange(model, with_({ then: [PHASE], sort: byCode })),
  phaseThenWbs: (model: Model) => arrange(model, with_({ by: PHASE, then: [null], sort: byCode })),
  roomThenTag: (model: Model) => arrange(model, with_({ by: ROOM, then: [TAG] })),
  roomThenWbs: (model: Model) => arrange(model, with_({ by: ROOM, then: [null] })),
};

/** The name of one piece of work at scale. */
export type Work = keyof typeof AT_SCALE;
