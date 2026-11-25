// The filter builder's conditions, and the `query` parameters they make:
// each condition is one of `query`'s filters, which the server applies, so
// the grid finds what `totalfloat query` would.

import type { QueryParams } from "../api";

/** One of `query`'s filters, by its argument's name. */
export type FilterField =
  | "code"
  | "wbs"
  | "resource"
  | "status"
  | "type"
  | "float_min"
  | "float_max"
  | "critical"
  | "constrained"
  | "start_from"
  | "start_to"
  | "finish_from"
  | "finish_to"
  | "text";

/** What a condition's value is entered as. */
export type ValueInput = "code" | "text" | "number" | "date" | "status" | "type" | "none";

export interface FilterChoice {
  id: FilterField;
  label: string;
  input: ValueInput;
  /** How the condition reads in a summary, given its value. */
  reads: (value: string) => string;
}

/** The filters the builder offers, in the order it lists them. */
export const FILTER_CHOICES: readonly FilterChoice[] = [
  { id: "code", label: "Activity code", input: "code", reads: (v) => v },
  { id: "wbs", label: "WBS", input: "text", reads: (v) => `WBS ${v}` },
  { id: "resource", label: "Resource", input: "text", reads: (v) => `resource ${v}` },
  { id: "status", label: "Status", input: "status", reads: (v) => v },
  { id: "type", label: "Activity type", input: "type", reads: (v) => v },
  { id: "float_min", label: "Total float at least (h)", input: "number", reads: (v) => `float ≥ ${v}h` },
  { id: "float_max", label: "Total float at most (h)", input: "number", reads: (v) => `float ≤ ${v}h` },
  { id: "critical", label: "Critical", input: "none", reads: () => "critical" },
  { id: "constrained", label: "Constrained", input: "none", reads: () => "constrained" },
  { id: "start_from", label: "Starts on or after", input: "date", reads: (v) => `start ≥ ${v}` },
  { id: "start_to", label: "Starts on or before", input: "date", reads: (v) => `start ≤ ${v}` },
  { id: "finish_from", label: "Finishes on or after", input: "date", reads: (v) => `finish ≥ ${v}` },
  { id: "finish_to", label: "Finishes on or before", input: "date", reads: (v) => `finish ≤ ${v}` },
  { id: "text", label: "ID or name contains", input: "text", reads: (v) => `“${v}”` },
];

export const CHOICE_BY_ID = new Map(FILTER_CHOICES.map((c) => [c.id, c]));

export const STATUS_VALUES = ["not-started", "in-progress", "completed"] as const;
export const TYPE_VALUES = ["task", "resource", "loe", "start-milestone", "finish-milestone", "wbs-summary"] as const;

/**
 * A filter and its value: `TYPE=VALUE` for a code, `YYYY-MM-DD` for a date,
 * nothing for a switch. Conditions with no `group` must all hold; those with
 * a group form `query`'s `any` groups, of which an activity meets one whole
 * group at least.
 */
export interface Condition {
  field: FilterField;
  value: string;
  /** The "any of" group it's in, from 0; none for a condition that must hold. */
  group?: number;
}

/** A filter's conditions as layouts and named filters keep them. */
export interface Built {
  built: Record<string, string>;
  any: Record<string, string>[];
}

/** Whether a condition has what it needs to be applied. */
export function complete(condition: Condition): boolean {
  const input = CHOICE_BY_ID.get(condition.field)?.input;
  if (input === "none") return true;
  const value = condition.value.trim();
  if (value === "") return false;
  if (input === "code") {
    const parts = codeParts(value);
    return parts.type.trim() !== "" && parts.value.trim() !== "";
  }
  if (input === "number") return Number.isFinite(Number(value));
  if (input === "date") return /^\d{4}-\d{2}-\d{2}$/.test(value);
  return true;
}

/**
 * The `query` parameters for complete conditions. A date range takes in the
 * whole day: from its first minute, to its last.
 */
export function paramsOf(conditions: readonly Condition[]): QueryParams {
  const { built, any } = builtOf(conditions);
  return any.length > 0 ? { ...built, any: JSON.stringify(any) } : built;
}

/** The "any of" groups' numbers, in order, among the conditions. */
export function groupsIn(conditions: readonly Condition[]): number[] {
  const groups = new Set<number>();
  for (const c of conditions) if (c.group !== undefined) groups.add(c.group);
  return [...groups].sort((a, b) => a - b);
}

/**
 * The complete conditions as a layout's or named filter's `built` and `any`:
 * a group with no complete condition is left out.
 */
export function builtOf(conditions: readonly Condition[]): Built {
  const built = flatParams(conditions.filter((c) => c.group === undefined));
  const any = groupsIn(conditions)
    .map((group) => flatParams(conditions.filter((c) => c.group === group)))
    .filter((params) => Object.keys(params).length > 0);
  return { built, any };
}

/** One set of conditions as `query` parameters, ignoring their groups. */
function flatParams(conditions: readonly Condition[]): Record<string, string> {
  const params: Record<string, string> = {};
  for (const condition of conditions) {
    if (!complete(condition)) continue;
    const value = condition.value.trim();
    switch (CHOICE_BY_ID.get(condition.field)?.input) {
      case "none":
        params[condition.field] = "true";
        break;
      case "date":
        params[condition.field] = `${value} ${condition.field.endsWith("_to") ? "23:59" : "00:00"}`;
        break;
      default:
        params[condition.field] = value;
    }
  }
  return params;
}

/**
 * The conditions `query` parameters stand for, as a layout keeps them: the
 * inverse of `paramsOf`, in the builder's order. Names `query` doesn't take
 * are left out.
 */
export function conditionsOf(
  params: Readonly<Record<string, string | undefined>>,
  any: readonly Readonly<Record<string, string | undefined>>[] = [],
): Condition[] {
  return [
    ...flatConditions(params),
    ...any.flatMap((group, at) => flatConditions(group).map((c) => ({ ...c, group: at }))),
  ];
}

function flatConditions(params: Readonly<Record<string, string | undefined>>): Condition[] {
  const conditions: Condition[] = [];
  for (const choice of FILTER_CHOICES) {
    const value = params[choice.id];
    if (value === undefined) continue;
    switch (choice.input) {
      case "none":
        conditions.push({ field: choice.id, value: "" });
        break;
      case "date":
        conditions.push({ field: choice.id, value: value.slice(0, 10) });
        break;
      default:
        conditions.push({ field: choice.id, value });
    }
  }
  return conditions;
}

/** The conditions, as they read in the toolbar: groups as "any of (…; …)". */
export function describe(conditions: readonly Condition[]): string {
  const reads = (these: readonly Condition[]) =>
    these
      .filter(complete)
      .map((c) => CHOICE_BY_ID.get(c.field)?.reads(c.value.trim()) ?? c.field)
      .join(", ");
  const all = reads(conditions.filter((c) => c.group === undefined));
  const groups = groupsIn(conditions)
    .map((group) => reads(conditions.filter((c) => c.group === group)))
    .filter((read) => read !== "");
  const any = groups.length > 0 ? `any of (${groups.join("; ")})` : "";
  return [all, any].filter((part) => part !== "").join(", ");
}

/** The filters not yet among one group's conditions: `query` takes each once a group. */
export function unused(conditions: readonly Condition[], group?: number): FilterChoice[] {
  const used = new Set(conditions.filter((c) => c.group === group).map((c) => c.field));
  return FILTER_CHOICES.filter((c) => !used.has(c.id));
}

/** A name as one item of a list parameter such as `columns`: its commas and backslashes escaped. */
export function listItem(name: string): string {
  return name.replace(/[\\,]/g, (c) => `\\${c}`);
}

/** A code condition's value, `TYPE=VALUE`, as its type and value. */
export function codeParts(value: string): { type: string; value: string } {
  const at = value.indexOf("=");
  return at < 0 ? { type: value, value: "" } : { type: value.slice(0, at), value: value.slice(at + 1) };
}
