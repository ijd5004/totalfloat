// Changes to the schedule's shape, as the grid shows them while they wait:
// activities added show as rows of their own, and those deleted go. Nothing
// here schedules anything; a row added has no dates until it's recorded.

import type { Edit, Rows } from "../api";
import { type WbsRef, withRefs } from "../grid/model";
import type { Change } from "./editing";
import { span } from "./cells";

/** An activity typed into a new row. */
export interface NewActivity {
  code: string;
  name: string;
  /** Its WBS node's path, as the rows give it. */
  wbs: string;
  /** As typed: `10d`, `40h`, a bare number in the units shown, or empty. */
  duration: string;
}

/** The key a change to the shape is dropped by. */
export const changeKey = (what: string, code: string) => `change:${what}:${code}`;

/**
 * The edit that adds `activity`, or why it can't be sent. Work needs a
 * duration, and 0 adds a milestone; `wbs` is how the server
 * knows the node.
 */
export function addEdit(activity: NewActivity, wbs: string, bare: "h" | "d"): Edit | string {
  const code = activity.code.trim();
  const name = activity.name.trim();
  if (code === "") return "type an Activity ID";
  if (name === "") return "type a name";
  const typed = activity.duration.trim();
  if (typed === "") return "type a duration, or 0 for a milestone";
  const duration = span(typed, bare);
  if (typeof duration !== "string") return duration.message;
  const none = Number(duration.slice(0, -1)) === 0;
  return none
    ? { op: "add_activity", code, name, wbs, activity_type: "finish-milestone" }
    : { op: "add_activity", code, name, wbs, duration };
}

/** The hours a typed duration comes to, with `perDay` hours in a day, for showing it before it's recorded. */
function hoursOf(typed: string, bare: "h" | "d", perDay: number): number {
  const duration = span(typed, bare);
  if (typeof duration !== "string") return 0;
  const amount = Number(duration.slice(0, -1));
  const unit = duration.slice(-1);
  const days = { d: 1, w: 5, m: 21, y: 250 }[unit as "d" | "w" | "m" | "y"];
  return unit === "h" ? amount : amount * (days ?? 1) * perDay;
}

/**
 * The rows as they'd be with `changes`: each activity added after the rest,
 * with a negative `task_id` and no dates, and each one deleted left out.
 */
export function rowsWith(rows: Rows, changes: readonly Change[] | undefined, bare: "h" | "d"): Rows {
  if (!changes || changes.length === 0) return rows;
  // Deleted by ID, in its project where the change names one.
  const deleted = new Set(changes.flatMap((c) => (c.edit.op === "delete_activity" ? [`${c.project ?? ""}:${c.edit.activity}`] : [])));
  const gone = (i: number) => deleted.has(`:${rows.code[i] ?? ""}`) || deleted.has(`${rows.project[i] ?? ""}:${rows.code[i] ?? ""}`);
  const added = changes.flatMap((c) => (c.edit.op === "add_activity" && c.adds ? [c] : []));
  // Moved to another WBS node, by ID, in its project where the change names one.
  const moved = new Map(changes.flatMap((c) => (c.edit.op === "set_wbs" && c.moves !== undefined ? [[`${c.project ?? ""}:${c.edit.activity}`, c.moves] as const] : [])));
  if (deleted.size === 0 && added.length === 0 && moved.size === 0) return rows;
  const keep: number[] = [];
  for (let i = 0; i < rows.count; i++) if (!gone(i)) keep.push(i);
  const out: Record<string, unknown> = { ...rows };
  // Every per-activity column, kept rows first, then a value for each added one.
  for (const [field, column] of Object.entries(rows)) {
    if (!Array.isArray(column) || column.length !== rows.count) continue;
    const next: unknown[] = keep.map((i) => column[i]);
    added.forEach((change, k) => next.push(addedValue(field, change, k, rows, bare)));
    out[field] = next;
  }
  out.count = keep.length + added.length;
  if (moved.size > 0) {
    const codes = out.code as string[];
    const projects = out.project as number[];
    out.wbs_path = (out.wbs_path as string[]).map(
      (path, k) => moved.get(`${projects[k] ?? ""}:${codes[k] ?? ""}`) ?? moved.get(`:${codes[k] ?? ""}`) ?? path,
    );
  }
  return out as unknown as Rows;
}

/** A column's value for the `k`th activity added. */
function addedValue(field: string, change: Change, k: number, rows: Rows, bare: "h" | "d"): unknown {
  const adds = change.adds as NewActivity;
  // As its WBS's activities are: their project and hours in a day.
  const near = rows.wbs_path.findIndex((path) => path === adds.wbs);
  const perDay = (near >= 0 ? rows.hours_per_day?.[near] : rows.hours_per_day?.[0]) ?? 8;
  const hours = hoursOf(adds.duration, bare, perDay);
  switch (field) {
    case "id":
      return -(k + 1);
    case "project":
      return change.project ?? rows.project[near >= 0 ? near : 0] ?? 0;
    case "code":
      return adds.code.trim();
    case "name":
      return adds.name.trim();
    case "wbs_path":
      return adds.wbs;
    case "type":
      return change.edit.op === "add_activity" && change.edit.activity_type ? change.edit.activity_type : "task";
    case "status":
      return "not-started";
    case "critical":
      return false;
    case "original_duration_hours":
    case "remaining_duration_hours":
      return hours;
    case "hours_per_day":
      return perDay;
    default:
      return null;
  }
}

/** The WBS as the grid shows it: each node's place among its siblings, name and reference for an edit. */
export interface WbsShown {
  order: ReadonlyMap<string, number> | null;
  names: ReadonlyMap<string, string> | null;
  refs: ReadonlyMap<string, WbsRef> | null;
}

/**
 * The WBS with `changes`: each node added after its siblings, with its name
 * and code, and each renamed node's new name and code. A node keeps its path
 * while a new code waits, as its activities' do; edits name it by the new one.
 */
export function wbsWith(shown: WbsShown, changes: readonly Change[] | undefined): WbsShown {
  const made = (changes ?? []).filter((c) => c.wbs !== undefined && (c.edit.op === "add_wbs" || c.edit.op === "rename_wbs"));
  if (made.length === 0 || !shown.order || !shown.refs) return shown;
  const order = new Map(shown.order);
  const names = new Map(shown.names ?? []);
  const refs = new Map(shown.refs);
  let next = 0;
  for (const at of order.values()) next = Math.max(next, at + 1);
  for (const { edit, wbs, project } of made) {
    if (!wbs) continue;
    const was = refs.get(wbs.path);
    if (edit.op === "add_wbs") {
      order.set(wbs.path, next++);
      refs.set(wbs.path, { project: project ?? was?.project ?? 0, code: wbs.code ?? "", ref: null });
    } else if (was && wbs.code !== undefined) {
      refs.set(wbs.path, { ...was, code: wbs.code });
    }
    if (wbs.name !== undefined) names.set(wbs.path, wbs.name);
  }
  return { order, names, refs: withRefs(refs) };
}

/**
 * Why a WBS code and name typed for a node under `parent` can't be sent, or
 * null; `self` is the node being renamed. The server checks the rest.
 */
export function wbsProblem(
  refs: ReadonlyMap<string, WbsRef> | null,
  code: string,
  name: string,
  parent: string | null,
  self: string | null,
): string | null {
  if (code === "") return "type a code";
  if (name === "") return "type a name";
  // The grid finds a node by its codes joined with dots.
  if (code.includes(".")) return "a WBS code here can't hold a dot, which joins codes in its path";
  const lower = code.toLowerCase();
  for (const [path, node] of refs ?? []) {
    if (path === self) continue;
    const above = path.includes(".") ? path.slice(0, path.lastIndexOf(".")) : null;
    if (above === parent && node.code.toLowerCase() === lower) return `${code} is already under that node`;
  }
  return null;
}
