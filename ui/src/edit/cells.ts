// Which grid cells can be edited, and how typed text becomes edit commands
// for `/api/1/edit`. Everything here is checked before anything is sent, so a
// value the server could never take is marked on its cell at once.

import type { Edit, Rows } from "../api";
import { COLUMN_BY_ID, type ColumnId, hours, unitsShown } from "../grid/columns";

/** A cell, by the activity's `task_id` and the column: `1234:original`. */
export type CellKey = string;

export function cellKey(id: number, column: ColumnId): CellKey {
  return `${id}:${column}`;
}

/** Text typed or pasted into a cell. */
export interface Typed {
  /** The activity's `task_id`, which holds across fresh rows. */
  id: number;
  column: ColumnId;
  text: string;
  /**
   * For a duration, the unit a bare number meant when it was typed: the one
   * shown then. Kept, so choosing other units later doesn't change it.
   */
  unit?: "h" | "d";
}

/** Why a cell's value was refused, and what to do. */
export interface CellProblem {
  message: string;
  hint: string;
}

/** Edits to send, and the cells each came from, by its place in the batch. */
export interface Batch {
  edits: Edit[];
  cells: CellKey[][];
  /** The project, when every edit is in one. */
  project?: number;
}

/**
 * A batch, or the cells refused. Either way, `kept` is the typed cells that
 * change something or were refused: cells left as they were aren't kept, so
 * they can't later undo someone else's change.
 */
export type Built = ({ batch: Batch } | { refused: Map<CellKey, CellProblem> }) & { kept: Typed[] };

/** The constraint kinds, as the server names them. */
export const CONSTRAINT_KINDS = [
  "start-on",
  "start-on-or-after",
  "start-on-or-before",
  "finish-on",
  "finish-on-or-after",
  "finish-on-or-before",
  "as-late-as-possible",
  "mandatory-start",
  "mandatory-finish",
] as const;
type ConstraintKind = (typeof CONSTRAINT_KINDS)[number];

/** How each editable column is typed in. */
export type InputKind = "text" | "span" | "date" | "calendar" | "constraint";

const INPUT: Partial<Record<ColumnId, InputKind>> = {
  name: "text",
  original: "span",
  remaining: "span",
  calendar: "calendar",
  constraint: "constraint",
  constraint_date: "date",
  actual_start: "date",
  actual_finish: "date",
};

/** How a column is typed in, or undefined if it can't be edited here. */
export function inputKind(column: ColumnId): InputKind | undefined {
  return INPUT[column];
}

/** The text an input over a cell starts with: durations carry their unit, and dates are as the server writes them. */
export function inputText(rows: Rows, i: number, column: ColumnId): string {
  if (column === "original") return spanInput(rows.original_duration_hours[i], rows.hours_per_day?.[i]);
  if (column === "remaining") return spanInput(rows.remaining_duration_hours[i], rows.hours_per_day?.[i]);
  const shown = COLUMN_BY_ID.get(column);
  return shown?.raw?.(rows, i) ?? shown?.text(rows, i) ?? "";
}

/** A span as an input starts with it: in days where they're shown and exact, else hours. */
function spanInput(value: number | undefined, perDay: number | null | undefined): string {
  const h = value ?? 0;
  if (unitsShown() === "days" && perDay) {
    const d = Math.round((h / perDay) * 1000) / 1000;
    if (Math.abs(d * perDay - h) < 1e-6) return `${d}d`;
  }
  return `${hours(h)}h`;
}

/** The unit a bare number means for activity `i`: the one its spans are shown in. */
function bareUnit(rows: Rows, i: number): "h" | "d" {
  return unitsShown() === "days" && rows.hours_per_day?.[i] ? "d" : "h";
}

/**
 * Hours as a span the server keeps to the same minute: three decimals of an
 * hour are within two seconds, and the server rounds to the minute.
 */
function exactHours(value: number): string {
  return `${Math.round(value * 1000) / 1000}h`;
}

/** A typed cell's text as shown while it waits: a bare duration with the unit it was typed in. */
export function shownText(cell: Typed): string {
  const text = cell.text.trim();
  return cell.unit && PLAIN_NUMBER.test(text) ? `${text}${cell.unit}` : cell.text;
}

/** Typed cells with the unit each duration's bare number means now, unless it has one already. */
export function withUnits(rows: Rows, typed: readonly Typed[]): Typed[] {
  return typed.map((cell) => {
    if (cell.unit || (cell.column !== "original" && cell.column !== "remaining")) return cell;
    const i = indexOf(rows, cell.id);
    return i === undefined ? cell : { ...cell, unit: bareUnit(rows, i) };
  });
}

// The span pattern from command-1.schema.json.
const SPAN = /^-?[0-9]*\.?[0-9]{0,3}[hHdDwWmMyY]$/;
const PLAIN_NUMBER = /^-?[0-9]*\.?[0-9]+$/;
const DATE = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2})$/;

/** A duration as the API takes it; a bare number is in `bare`, the unit the grid shows. */
export function span(text: string, bare: "h" | "d" = "h"): string | CellProblem {
  const typed = text.trim().replace(/\s+/g, "");
  const value = PLAIN_NUMBER.test(typed) ? `${typed}${bare}` : typed;
  if (SPAN.test(value) && /[0-9]/.test(value)) return value.toLowerCase();
  return {
    message: `"${text.trim()}" isn't a duration`,
    hint: "type hours, days, weeks, months or years, such as 40, 40h, 5d, 2w or 1m",
  };
}

/** A date and time as the API takes it, `YYYY-MM-DD HH:MM`. */
export function dateTime(text: string): string | CellProblem {
  const typed = text.trim().replace(/\s+/g, " ").replace("T", " ");
  const match = DATE.exec(typed);
  if (match) {
    const [, y, mo, d, h, mi] = match.map(Number) as [number, number, number, number, number, number];
    const day = new Date(Date.UTC(y, mo - 1, d));
    if (day.getUTCMonth() === mo - 1 && day.getUTCDate() === d && h <= 23 && mi <= 59) return typed;
  }
  return {
    message: `"${text.trim()}" isn't a date and time`,
    hint: "type it as 2026-03-02 08:00",
  };
}

/** A constraint kind from what was typed, in the server's words or a person's. */
export function constraintKind(text: string): ConstraintKind | null | CellProblem {
  const typed = text.trim().toLowerCase().replace(/[\s_]+/g, "-");
  if (typed === "" || typed === "none") return null;
  const found = CONSTRAINT_KINDS.find((kind) => kind === typed);
  if (found) return found;
  return {
    message: `"${text.trim()}" isn't a kind of constraint`,
    hint: `choose one of ${CONSTRAINT_KINDS.join(", ")}, or leave it empty for none`,
  };
}

function isProblem<T>(value: T | CellProblem): value is CellProblem {
  return typeof value === "object" && value !== null && "message" in value && "hint" in value;
}

const indexes = new WeakMap<Rows, Map<number, number>>();

/** Where activity `id` is in `rows`. */
export function indexOf(rows: Rows, id: number): number | undefined {
  let map = indexes.get(rows);
  if (!map) {
    map = new Map(rows.id.map((value, i) => [value, i]));
    indexes.set(rows, map);
  }
  return map.get(id);
}

/** What one activity's typed cells add up to. */
interface Changes {
  i: number;
  name?: [string, CellKey];
  calendar?: [string, CellKey];
  original?: [string, CellKey];
  remaining?: [string, CellKey];
  kind?: [ConstraintKind | null, CellKey];
  date?: [string | null, CellKey];
  start?: [string, CellKey];
  finish?: [string, CellKey];
}

/**
 * The edits typed cells make, one batch, all or nothing: any cell that can't
 * be sent refuses the lot, and each such cell says why. Cells left as they
 * were are dropped, so copying a row and pasting it back changes only what
 * differs.
 */
export function buildBatch(rows: Rows, typed: readonly Typed[]): Built {
  const refused = new Map<CellKey, CellProblem>();
  const byActivity = new Map<number, Changes>();
  const kept: Typed[] = [];
  // Work not started whose original duration is typed and changes: there,
  // remaining follows original unless given, so a remaining typed with it is
  // sent even if it's as it was.
  // Activities given another calendar: a duration in days typed with it
  // counts that calendar's days, so it's sent even if it matches now.
  const recalendared = new Set(
    typed
      .filter(({ id, column, text }) => {
        const i = column === "calendar" ? indexOf(rows, id) : undefined;
        return i !== undefined && text.trim() !== "" && text.trim() !== (rows.calendar[i] ?? "");
      })
      .map(({ id }) => id),
  );
  const newOriginal = new Set(
    typed
      .filter((cell) => {
        const i = cell.column === "original" ? indexOf(rows, cell.id) : undefined;
        return i !== undefined && rows.status[i] === "not-started" && !unchanged(rows, i, cell, recalendared.has(cell.id));
      })
      .map(({ id }) => id),
  );
  for (const cell of typed) {
    const { id, column, text } = cell;
    const key = cellKey(id, column);
    const i = indexOf(rows, id);
    if (id < 0) {
      refused.set(key, { message: "this activity is added once you apply it", hint: "apply, then change it" });
      kept.push(cell);
      continue;
    }
    if (i === undefined) {
      refused.set(key, { message: "this activity is no longer in the schedule", hint: "it may have been deleted; reload to see" });
      kept.push(cell);
      continue;
    }
    const kind = inputKind(column);
    // As the input opened, or as the grid shows and copies it.
    const same = unchanged(rows, i, cell, recalendared.has(id));
    if (same && !(column === "remaining" && newOriginal.has(id))) continue;
    // A remaining kept as it was while the original changes is sent, and
    // kept, as the hours stored, so copying it from the grid can't round it.
    kept.push(same ? { id, column, text: exactHours(rows.remaining_duration_hours[i] ?? 0) } : cell);
    const title = COLUMN_BY_ID.get(column)?.title ?? column;
    if (!kind) {
      refused.set(key, { message: `${title} can't be changed here`, hint: "it comes from the schedule's calculation or its structure" });
      continue;
    }
    const changes = byActivity.get(id) ?? { i };
    byActivity.set(id, changes);
    const take = <T>(value: T | CellProblem, put: (value: T) => void) => {
      if (isProblem(value)) refused.set(key, value);
      else put(value);
    };
    switch (column) {
      case "name":
        if (text.trim() === "") refused.set(key, { message: "an activity needs a name", hint: "type a name" });
        else changes.name = [text.trim(), key];
        break;
      case "calendar":
        if (text.trim() === "") refused.set(key, { message: "an activity needs a calendar", hint: "choose one from the list" });
        else changes.calendar = [text.trim(), key];
        break;
      case "original":
        take(span(text, cell.unit ?? bareUnit(rows, i)), (value) => (changes.original = [value, key]));
        break;
      case "remaining":
        if (same) {
          // Kept as it is while the original changes: sent as stored, not as
          // the grid rounds it.
          changes.remaining = [exactHours(rows.remaining_duration_hours[i] ?? 0), key];
        } else {
          take(span(text, cell.unit ?? bareUnit(rows, i)), (value) => (changes.remaining = [value, key]));
        }
        break;
      case "constraint":
        take(constraintKind(text), (value) => (changes.kind = [value, key]));
        break;
      case "constraint_date":
        if (text.trim() === "") changes.date = [null, key];
        else take(dateTime(text), (value) => (changes.date = [value, key]));
        break;
      case "actual_start":
      case "actual_finish": {
        if (text.trim() === "") {
          refused.set(key, {
            message: `${title} can't be emptied here`,
            hint: "to take progress away, use `totalfloat edit --clear-progress`",
          });
          break;
        }
        take(dateTime(text), (value) => {
          if (column === "actual_start") changes.start = [value, key];
          else changes.finish = [value, key];
        });
        break;
      }
      default:
        break;
    }
  }

  const edits: Edit[] = [];
  const cells: CellKey[][] = [];
  const projects = new Set<number>();
  const add = (edit: Edit, from: CellKey[]) => {
    edits.push(edit);
    cells.push(from);
  };
  for (const changes of byActivity.values()) {
    const { i } = changes;
    const activity = rows.code[i] ?? "";
    projects.add(rows.project[i] ?? 0);
    // The calendar first: durations, in progress or not, are read in it.
    if (changes.calendar) add({ op: "set_calendar", activity, calendar: changes.calendar[0] }, [changes.calendar[1]]);
    // Remaining typed with actuals goes with them, in one progress edit: on
    // its own after an actual finish, it would land on finished work.
    const progress = changes.start || changes.finish;
    if (progress) {
      add(
        {
          op: "set_progress",
          activity,
          ...(changes.start ? { actual_start: changes.start[0] } : {}),
          ...(changes.finish ? { actual_finish: changes.finish[0] } : {}),
          ...(changes.remaining ? { remaining: changes.remaining[0] } : {}),
        },
        [changes.start?.[1], changes.finish?.[1], changes.remaining?.[1]].filter((k) => k !== undefined),
      );
    }
    if (changes.name) add({ op: "set_name", activity, name: changes.name[0] }, [changes.name[1]]);
    if (changes.kind || changes.date) {
      const constraint = constraintEdit(rows, i, activity, changes, refused);
      if (constraint) add(constraint.edit, constraint.cells);
    }
    const remaining = progress ? undefined : changes.remaining;
    if (changes.original || remaining) {
      add(
        {
          op: "set_duration",
          activity,
          ...(changes.original ? { original: changes.original[0] } : {}),
          ...(remaining ? { remaining: remaining[0] } : {}),
        },
        [changes.original?.[1], remaining?.[1]].filter((k) => k !== undefined),
      );
    }
  }
  // Edits name activities by ID, and a request names at most one project, so
  // across projects an ID that's in more than one can't be told apart.
  if (projects.size > 1) {
    const where = new Map<string, Set<number>>();
    rows.code.forEach((code, i) => {
      const set = where.get(code) ?? new Set<number>();
      set.add(rows.project[i] ?? 0);
      where.set(code, set);
    });
    edits.forEach((edit, n) => {
      const activity = "activity" in edit ? (edit.activity ?? "") : "";
      if ((where.get(activity)?.size ?? 0) < 2) return;
      for (const key of cells[n] ?? []) {
        refused.set(key, {
          message: `${activity} is an activity ID in more than one project`,
          hint: "edit one project at a time when IDs repeat across projects",
        });
      }
    });
  }
  if (refused.size > 0) return { refused, kept };
  const [only] = projects;
  return { batch: { edits, cells, ...(projects.size === 1 && only !== undefined ? { project: only } : {}) }, kept };
}

/** The primary constraint a typed kind and date make, with what's already set filling the gaps. */
function constraintEdit(
  rows: Rows,
  i: number,
  activity: string,
  changes: Changes,
  refused: Map<CellKey, CellProblem>,
): { edit: Edit; cells: CellKey[] } | null {
  const cells = [changes.kind?.[1], changes.date?.[1]].filter((k) => k !== undefined);
  const kind = changes.kind ? changes.kind[0] : (rows.constraint[i] as ConstraintKind | null) ?? null;
  const date = changes.date ? changes.date[0] : (rows.constraint_date[i] ?? null);
  const blame = changes.date?.[1] ?? changes.kind?.[1] ?? "";
  if (kind === null) {
    if (changes.date && changes.date[0] !== null) {
      refused.set(
        blame,
        changes.kind
          ? { message: "with no constraint, there's no date to set", hint: "leave the date empty, or choose a constraint" }
          : { message: "there's no constraint to date", hint: "choose a constraint first" },
      );
      return null;
    }
    return { edit: { op: "clear_constraint", activity }, cells };
  }
  if (kind === "as-late-as-possible") {
    if (changes.date && changes.date[0] !== null) {
      refused.set(changes.date[1], { message: "as late as possible takes no date", hint: "leave the date empty" });
      return null;
    }
    return { edit: { op: "set_constraint", activity, constraint: kind, date: null }, cells };
  }
  if (date === null) {
    refused.set(changes.date?.[1] ?? changes.kind?.[1] ?? blame, {
      message: `${kind} needs a date`,
      hint: "give the constraint date too, or clear the constraint",
    });
    return null;
  }
  return { edit: { op: "set_constraint", activity, constraint: kind, date }, cells };
}

/**
 * Whether a typed cell is its value as the input opened, or as the grid shows
 * and copies it. `recalendared` says the activity is given another calendar
 * in the same batch.
 */
function unchanged(rows: Rows, i: number, cell: Typed, recalendared: boolean): boolean {
  const { column, text } = cell;
  const opened = inputText(rows, i, column).trim();
  const shown = (COLUMN_BY_ID.get(column)?.text(rows, i) ?? "").trim();
  if (column === "original" || column === "remaining") {
    const unit = cell.unit ?? bareUnit(rows, i);
    const typed = span(text, unit);
    if (typeof typed !== "string") return false;
    // Days on a new calendar are new hours: sent as typed.
    if (recalendared && typed.endsWith("d")) return false;
    // As shown and copied, in the units shown when it was typed, even where
    // the grid rounds it: pasting a copied cell back changes nothing.
    if (text.trim() === opened || (unit === bareUnit(rows, i) && text.trim() === shown)) return true;
    // The same length however it's written (40, 40h or 5d), to the minute
    // the server keeps.
    const now = column === "original" ? rows.original_duration_hours[i] : rows.remaining_duration_hours[i];
    const perDay = rows.hours_per_day?.[i];
    if (now === undefined) return false;
    // Weeks, months and years need the calendar's hours in each, which the
    // grid doesn't hold: the server works them out.
    if (/[wmy]$/.test(typed)) return false;
    const amount = Number(typed.slice(0, -1));
    const inHours = typed.endsWith("d") ? (perDay ? amount * perDay : Number.NaN) : amount;
    return Math.round(inHours * 60) === Math.round(now * 60);
  }
  return text.trim() === opened || text.trim() === shown;
}

/** The calendars the rows use, for the picker. */
export function calendarsOf(rows: Rows): string[] {
  return [...new Set(rows.calendar.filter((name): name is string => !!name))].sort((a, b) => a.localeCompare(b));
}
