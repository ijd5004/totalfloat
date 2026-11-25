// Grouping by the activity's own fields, worked out from the rows the grid
// already has: status, activity type, calendar, constraint, and the month
// work starts or finishes. Codes and user fields come from the
// server instead.

import type { Grouping, Model } from "./model";

/** A grouping the grid works out itself. */
export interface BuiltInGroup {
  /** What a layout's `group` says. */
  field: string;
  /** What the Group menu and the bands say. */
  title: string;
}

export const BUILT_IN_GROUPS: readonly BuiltInGroup[] = [
  { field: "status", title: "Status" },
  { field: "type", title: "Activity type" },
  { field: "calendar", title: "Calendar" },
  { field: "constraint", title: "Constraint" },
  { field: "start_month", title: "Start month" },
  { field: "finish_month", title: "Finish month" },
];

export function isBuiltInGroup(field: string | null): boolean {
  return field !== null && BUILT_IN_GROUPS.some((g) => g.field === field);
}

const STATUSES: readonly [string, string][] = [
  ["not-started", "Not started"],
  ["in-progress", "In progress"],
  ["completed", "Completed"],
];

const TYPES: readonly [string, string][] = [
  ["task", "Task dependent"],
  ["resource", "Resource dependent"],
  ["loe", "Level of effort"],
  ["start-milestone", "Start milestone"],
  ["finish-milestone", "Finish milestone"],
  ["wbs-summary", "WBS summary"],
];

/** Labels by value, and the bands' order: the order listed. */
function labelled(pairs: readonly [string, string][]): { label: Map<string, string>; order: Map<string, number> } {
  return {
    label: new Map(pairs),
    order: new Map(pairs.map(([, label], k) => [label, k])),
  };
}

const STATUS = labelled(STATUSES);
const TYPE = labelled(TYPES);

/**
 * A built-in grouping of the rows. Bands go: status as work proceeds (not
 * started, in progress, completed); activity type in a fixed order (task
 * dependent, resource dependent, level of effort, start milestone, finish
 * milestone, WBS summary); calendar and constraint in natural order of
 * their names; months earliest first. Each ends with a band for activities
 * with no value. A month is the start or finish the Gantt shows: actual,
 * else early. Null for a field that isn't built in.
 */
export function builtInGrouping(model: Model, field: string): Grouping | null {
  const group = BUILT_IN_GROUPS.find((g) => g.field === field);
  if (!group) return null;
  const { rows } = model;
  const values = new Map<number, string>();
  let order: Map<string, number> | undefined;
  const valueOf: (i: number) => string | null | undefined = (() => {
    switch (field) {
      case "status":
        order = STATUS.order;
        return (i) => {
          const value = rows.status[i];
          return value === undefined ? null : (STATUS.label.get(value) ?? value);
        };
      case "type":
        order = TYPE.order;
        return (i) => {
          const value = rows.type[i];
          return value === undefined ? null : (TYPE.label.get(value) ?? value);
        };
      case "calendar":
        return (i) => rows.calendar[i];
      case "constraint":
        return (i) => rows.constraint[i];
      case "start_month":
        return (i) => model.starts[i]?.slice(0, 7);
      default:
        return (i) => model.finishes[i]?.slice(0, 7);
    }
  })();
  for (let i = 0; i < model.count; i++) {
    const value = valueOf(i);
    if (value) values.set(rows.id[i] ?? -1, value);
  }
  return { field, title: group.title, values, ...(order ? { order } : {}) };
}
