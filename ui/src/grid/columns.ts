// The grid's columns: what each shows and how it sorts. Dates show exactly as
// the server sends them, with no time-zone conversion (ARCHITECTURE.md, Data model); durations
// and float show in days of each activity's calendar,
// or in hours by choice; the server gives each activity's hours in a day.

import type { Rows } from "../api";
import { fieldWords } from "../history/words";
import { dateText, moveText } from "../dates";

export type ColumnId =
  | "code"
  | "name"
  | "original"
  | "remaining"
  | "start"
  | "finish"
  | "float"
  | "calendar"
  | "wbs"
  | "actual_start"
  | "actual_finish"
  | "constraint"
  | "constraint_date"
  | "before"
  | "after";

export interface Column {
  id: ColumnId;
  /** The full name: the column picker, the header's tooltip and screen readers use it. */
  title: string;
  /** What the header shows where the full name wouldn't fit its default width. */
  short?: string;
  /** Default width in CSS pixels. */
  width: number;
  align: "left" | "right";
  /** The cell's text for activity `i`. */
  text: (rows: Rows, i: number) => string;
  /** For a date, the server's own text, `YYYY-MM-DD HH:MM`, which an input over the cell starts with. */
  raw?: (rows: Rows, i: number) => string;
  /** The value sorted on: a number, or text compared in natural order. Null sorts last. */
  key: (rows: Rows, i: number) => number | string | null;
}

/** Days or hours: how durations and float are shown. */
export type Units = "days" | "hours";
let shown: Units = "days";

/** How durations and float are shown now. */
export function unitsShown(): Units {
  return shown;
}

/** Shows durations and float in `units` from now on. */
export function showUnits(units: Units): void {
  shown = units;
}

/** The unit's letter for a column title. */
function unit(): string {
  return shown === "days" ? "d" : "h";
}

/**
 * A span or float as shown: in days of the activity's calendar, or in hours.
 * Where the calendar's day isn't known, it's hours, marked `h`, so it can't
 * be read as days.
 */
export function spanText(hoursValue: number | null | undefined, perDay: number | null | undefined): string {
  if (hoursValue === null || hoursValue === undefined) return "";
  if (shown === "hours") return hours(hoursValue);
  if (!perDay) return `${hours(hoursValue)}h`;
  return hours(hoursValue / perDay);
}

/** A span or float in words, for tooltips: "2 days", "1 day", or "16 h" where the day isn't known or hours are chosen. */
export function spanWords(hoursValue: number, perDay: number | null | undefined): string {
  if (shown === "days" && perDay) {
    const d = hours(hoursValue / perDay);
    return `${d} ${d === "1" || d === "-1" ? "day" : "days"}`;
  }
  return `${hours(hoursValue)} h`;
}

/** Hours as `query` prints them: up to two decimals, no trailing zeros. */
export function hours(value: number | null | undefined): string {
  if (value === null || value === undefined) return "";
  return String(Math.round(value * 100) / 100);
}

/** One field a comparison found changed on an activity. */
export interface ComparedField {
  field: string;
  from: string;
  to: string;
}

/** What a comparison of two points found, for the grid's Before and After columns. */
export interface Compared {
  /** Each project's code, by `proj_id`, as the comparison names projects by code. */
  projects: ReadonlyMap<number, string>;
  /** By `comparedKey(project code, activity code)`. */
  activities: ReadonlyMap<string, { added: boolean; fields: ComparedField[] }>;
}

let compared: Compared | null = null;

/** Shows a comparison in the Before and After columns from now on, or none. */
export function showCompared(next: Compared | null): void {
  compared = next;
}

/** An activity's key in a comparison. */
export function comparedKey(project: string, code: string): string {
  return `${project}\u0000${code}`;
}

/** The Before and After columns: shown only while there's a comparison to show. */
export const COMPARE_COLUMNS: ReadonlySet<ColumnId> = new Set(["before", "after"]);

/** A date as the server writes one. */
const DATE_VALUE = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/;

/** A comparison's value as shown: spans like `32h` in the units shown, dates in the format shown, the rest as given. */
export function valueText(value: string, perDay: number | null | undefined): string {
  const span = /^(-?\d+(?:\.\d+)?)h$/.exec(value);
  if (span) return spanWords(Number(span[1]), perDay);
  return DATE_VALUE.test(value) ? dateText(value) : value || "none";
}

/**
 * A changed value before and after, as shown: dates in the format shown,
 * with their times when they fall on the same day, so the move shows.
 */
export function valuePair(from: string, to: string, perDay: number | null | undefined): [string, string] {
  if (DATE_VALUE.test(from) && DATE_VALUE.test(to)) return moveText(from, to);
  return [valueText(from, perDay), valueText(to, perDay)];
}

/** What changed on activity `i`, before or after; `raw` gives the values as sent, to sort on. */
function comparedText(rows: Rows, i: number, side: "from" | "to", raw = false): string {
  if (!compared) return "";
  const project = compared.projects.get(rows.project[i] ?? -1) ?? "";
  const found = compared.activities.get(comparedKey(project, rows.code[i] ?? ""));
  if (!found) return "";
  if (found.added) return side === "to" ? "Added" : "";
  const perDay = rows.hours_per_day?.[i];
  return found.fields
    .map((f) => `${fieldWords(f.field)} ${raw ? f[side] : valuePair(f.from, f.to, perDay)[side === "from" ? 0 : 1]}`)
    .join("; ");
}

/** The start to show: actual if started, else early. */
function start(rows: Rows, i: number): string | null {
  return rows.actual_start[i] ?? rows.early_start[i] ?? null;
}

/** The finish to show: actual if finished, else early. */
function finish(rows: Rows, i: number): string | null {
  return rows.actual_finish[i] ?? rows.early_finish[i] ?? null;
}

export const COLUMNS: Column[] = [
  { id: "code", title: "Activity ID", width: 110, align: "left", text: (r, i) => r.code[i] ?? "", key: (r, i) => r.code[i] ?? null },
  { id: "name", title: "Activity name", width: 260, align: "left", text: (r, i) => r.name[i] ?? "", key: (r, i) => r.name[i] ?? null },
  {
    id: "before",
    title: "Before",
    width: 200,
    align: "left",
    text: (r, i) => comparedText(r, i, "from"),
    key: (r, i) => comparedText(r, i, "from", true) || null,
  },
  {
    id: "after",
    title: "After",
    width: 200,
    align: "left",
    text: (r, i) => comparedText(r, i, "to"),
    key: (r, i) => comparedText(r, i, "to", true) || null,
  },
  {
    id: "original",
    get title() {
      return `Original (${unit()})`;
    },
    get short() {
      return `Orig (${unit()})`;
    },
    width: 84,
    align: "right",
    text: (r, i) => spanText(r.original_duration_hours[i], r.hours_per_day?.[i]),
    key: (r, i) => r.original_duration_hours[i] ?? null,
  },
  {
    id: "remaining",
    get title() {
      return `Remaining (${unit()})`;
    },
    get short() {
      return `Rem (${unit()})`;
    },
    width: 92,
    align: "right",
    text: (r, i) => spanText(r.remaining_duration_hours[i], r.hours_per_day?.[i]),
    key: (r, i) => r.remaining_duration_hours[i] ?? null,
  },
  { id: "start", title: "Start", width: 116, align: "left", text: (r, i) => dateText(start(r, i)), raw: (r, i) => start(r, i) ?? "", key: start },
  { id: "finish", title: "Finish", width: 116, align: "left", text: (r, i) => dateText(finish(r, i)), raw: (r, i) => finish(r, i) ?? "", key: finish },
  {
    id: "float",
    get title() {
      return `Total float (${unit()})`;
    },
    get short() {
      return `Float (${unit()})`;
    },
    width: 100,
    align: "right",
    text: (r, i) => spanText(r.total_float_hours[i], r.hours_per_day?.[i]),
    key: (r, i) => r.total_float_hours[i] ?? null,
  },
  { id: "calendar", title: "Calendar", width: 180, align: "left", text: (r, i) => r.calendar[i] ?? "", key: (r, i) => r.calendar[i] ?? null },
  { id: "wbs", title: "WBS", width: 180, align: "left", text: (r, i) => r.wbs_path[i] ?? "", key: (r, i) => r.wbs_path[i] ?? null },
  {
    id: "actual_start",
    title: "Actual start",
    width: 116,
    align: "left",
    text: (r, i) => dateText(r.actual_start[i]),
    raw: (r, i) => r.actual_start[i] ?? "",
    key: (r, i) => r.actual_start[i] ?? null,
  },
  {
    id: "actual_finish",
    title: "Actual finish",
    width: 116,
    align: "left",
    text: (r, i) => dateText(r.actual_finish[i]),
    raw: (r, i) => r.actual_finish[i] ?? "",
    key: (r, i) => r.actual_finish[i] ?? null,
  },
  {
    id: "constraint",
    title: "Constraint",
    width: 140,
    align: "left",
    text: (r, i) => r.constraint[i] ?? "",
    key: (r, i) => r.constraint[i] ?? null,
  },
  {
    id: "constraint_date",
    title: "Constraint date",
    width: 124,
    align: "left",
    text: (r, i) => dateText(r.constraint_date[i]),
    raw: (r, i) => r.constraint_date[i] ?? "",
    key: (r, i) => r.constraint_date[i] ?? null,
  },
];

export const COLUMN_BY_ID = new Map(COLUMNS.map((column) => [column.id, column]));
