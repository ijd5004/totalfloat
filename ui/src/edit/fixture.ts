// Rows for tests: a few activities with the fields editing reads.
import type { Rows } from "../api";

/** Three not-started activities, A1 to A3 (`task_id` 1 to 3), unless `change` says otherwise. */
export function sample(change: Partial<Record<keyof Rows, unknown[]>> = {}): Rows {
  const codes = ["A1", "A2", "A3"];
  const each = <T>(value: T) => codes.map(() => value);
  return {
    schema: "totalfloat.rows/1",
    file: "plant.tfdb@main",
    count: codes.length,
    id: [1, 2, 3],
    project: each(7),
    code: codes,
    name: codes.map((c) => `Name ${c}`),
    type: each("task"),
    status: each("not-started"),
    critical: each(false),
    calendar: ["Five day", "Five day", "Seven day"],
    wbs_path: each("P"),
    original_duration_hours: each(40),
    remaining_duration_hours: each(40),
    total_float_hours: each(0),
    free_float_hours: each(0),
    early_start: each("2026-01-05 08:00"),
    early_finish: each("2026-01-09 17:00"),
    late_start: each(null),
    late_finish: each(null),
    actual_start: each(null),
    actual_finish: each(null),
    constraint: each(null),
    constraint_date: each(null),
    secondary_constraint: each(null),
    secondary_constraint_date: each(null),
    ...change,
  } as unknown as Rows;
}
