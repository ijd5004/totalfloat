// The activity the grid has selected, as the right column's panels follow it.

import type { Rows } from "../api";

/** The activity the grid has selected: its `task_id`, code and project, from the rows. */
export interface Selected {
  id: number;
  code: string;
  project: number | undefined;
}

/** The activity on `line` of the arranged `lines`, or null on a band or past the end. */
export function selectedIn(rows: Rows, lines: Int32Array, line: number): Selected | null {
  const i = lines[line];
  if (i === undefined || i < 0) return null;
  return { id: rows.id[i] ?? -1, code: rows.code[i] ?? "", project: rows.project[i] };
}
