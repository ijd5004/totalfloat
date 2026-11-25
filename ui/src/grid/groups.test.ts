import { describe, expect, it } from "vitest";

import type { Rows } from "../api";
import { BUILT_IN_GROUPS, builtInGrouping } from "./groups";
import { DEFAULT_ARRANGEMENT, NO_VALUE, arrange, buildModel, sorted } from "./model";

/** Six activities, each field with a gap, in no order. */
const ROWS = {
  schema: "totalfloat.rows/1",
  file: "plan.xer",
  count: 6,
  id: [1, 2, 3, 4, 5, 6],
  project: [1, 1, 1, 1, 1, 1],
  code: ["A6", "A5", "A4", "A3", "A2", "A1"],
  name: ["f", "e", "d", "c", "b", "a"],
  type: ["finish-milestone", "task", "loe", "task", "start-milestone", "resource"],
  status: ["completed", "not-started", "in-progress", "not-started", "completed", "in-progress"],
  critical: [false, false, false, false, false, false],
  calendar: ["Seven day", "Five day", "Five day 10", "Five day 2", null, "Five day"],
  wbs_path: ["P.A", "P.A", "P.B", "P.B", "P.A", "P.B"],
  original_duration_hours: [0, 8, 16, 24, 0, 40],
  remaining_duration_hours: [0, 8, 16, 24, 0, 40],
  total_float_hours: [0, 0, 0, 0, 0, 0],
  free_float_hours: [0, 0, 0, 0, 0, 0],
  early_start: ["2026-11-02 08:00", "2026-03-02 08:00", null, "2026-10-01 08:00", "2026-03-30 08:00", "2026-03-02 08:00"],
  early_finish: ["2026-11-02 08:00", "2026-03-06 17:00", null, "2026-10-01 17:00", "2026-03-30 08:00", "2026-04-10 17:00"],
  late_start: [null, null, null, null, null, null],
  late_finish: [null, null, null, null, null, null],
  actual_start: ["2026-02-02 08:00", null, null, null, "2026-01-05 08:00", null],
  actual_finish: ["2026-02-03 17:00", null, null, null, "2026-01-05 08:00", null],
  constraint: [null, "start-on-or-after", null, "finish-on-or-before", null, "start-on-or-after"],
  constraint_date: [null, null, null, null, null, null],
  secondary_constraint: [null, null, null, null, null, null],
  secondary_constraint_date: [null, null, null, null, null, null],
} as unknown as Rows;

const MODEL = buildModel(ROWS);

/** Each band's title and the codes under it, top to bottom. */
function bands(field: string): [string, string[]][] {
  const by = builtInGrouping(MODEL, field);
  const { lines, bands } = arrange(MODEL, { ...DEFAULT_ARRANGEMENT, sort: [{ column: "code", descending: false }], by });
  const out: [string, string[]][] = [];
  for (const line of lines) {
    if (line < 0) out.push([bands[-line - 1]?.label ?? "", []]);
    else out.at(-1)?.[1].push(ROWS.code[line] ?? "");
  }
  return out;
}

describe("grouping by an activity's own fields", () => {
  it("bands status as work proceeds", () => {
    expect(bands("status")).toEqual([
      ["Not started", ["A3", "A5"]],
      ["In progress", ["A1", "A4"]],
      ["Completed", ["A2", "A6"]],
    ]);
  });

  it("bands activity types in a fixed order", () => {
    expect(bands("type")).toEqual([
      ["Task dependent", ["A3", "A5"]],
      ["Resource dependent", ["A1"]],
      ["Level of effort", ["A4"]],
      ["Start milestone", ["A2"]],
      ["Finish milestone", ["A6"]],
    ]);
  });

  it("bands calendars and constraints by name, in natural order, then those with none", () => {
    expect(bands("calendar")).toEqual([
      ["Five day", ["A1", "A5"]],
      ["Five day 2", ["A3"]],
      ["Five day 10", ["A4"]],
      ["Seven day", ["A6"]],
      [NO_VALUE, ["A2"]],
    ]);
    expect(bands("constraint")).toEqual([
      ["finish-on-or-before", ["A3"]],
      ["start-on-or-after", ["A1", "A5"]],
      [NO_VALUE, ["A2", "A4", "A6"]],
    ]);
  });

  it("bands months earliest first, from actual dates where there are any", () => {
    expect(bands("start_month")).toEqual([
      ["2026-01", ["A2"]],
      ["2026-02", ["A6"]],
      ["2026-03", ["A1", "A5"]],
      ["2026-10", ["A3"]],
      [NO_VALUE, ["A4"]],
    ]);
    expect(bands("finish_month")).toEqual([
      ["2026-01", ["A2"]],
      ["2026-02", ["A6"]],
      ["2026-03", ["A5"]],
      ["2026-04", ["A1"]],
      ["2026-10", ["A3"]],
      [NO_VALUE, ["A4"]],
    ]);
  });

  it("offers every one, and nothing for a field that isn't built in", () => {
    expect(BUILT_IN_GROUPS.map((g) => g.field)).toEqual(["status", "type", "calendar", "constraint", "start_month", "finish_month"]);
    expect(builtInGrouping(MODEL, "code:Phase")).toBeNull();
    expect(builtInGrouping(MODEL, "status")?.title).toBe("Status");
  });
});

describe("sorting on three keys", () => {
  it("applies them in order", () => {
    const order = sorted(MODEL, Int32Array.from([0, 1, 2, 3, 4, 5]), [
      { column: "wbs", descending: false },
      { column: "original", descending: true },
      { column: "code", descending: false },
    ]);
    // P.A: 8h, then the two 0h by code; P.B: 40h, 24h, 16h.
    expect([...order].map((i) => ROWS.code[i])).toEqual(["A5", "A2", "A6", "A1", "A3", "A4"]);
  });
});
