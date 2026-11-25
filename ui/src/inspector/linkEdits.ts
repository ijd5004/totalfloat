// Relationship edits from the inspector and the Gantt, as the pending-edits
// flow takes them: `add_link`, `change_link` and `remove_link`, each with a
// key it waits under, so a second change to the same link replaces the
// first, and words for the Changes panel.

import type { Edit, Rows } from "../api";
import { type CellProblem, indexOf, span } from "../edit/cells";
import { unitsShown } from "../grid/columns";

export const LINK_TYPES = ["FS", "SS", "FF", "SF"] as const;
export type LinkType = (typeof LINK_TYPES)[number];

/** A link edit waiting to be applied. */
export interface LinkChange {
  /** What it waits under: `link:add:<pred>><succ>` for a new link, `link:<task_pred_id>` for one already there. */
  key: string;
  edit: Edit;
  /** What it does, as the Changes panel lists it. */
  words: string;
  /** The project it's in (`proj_id`), the successor's: a request names one at most. */
  project?: number;
}

/** The key a new link between two activities (by `task_id`) waits under. */
export function addKey(predecessor: number, successor: number): string {
  return `link:add:${predecessor}>${successor}`;
}

/** The key a change to a link already there (by `task_pred_id`) waits under. */
export function linkKey(link: number): string {
  return `link:${link}`;
}

/** Whether a waiting change is a link edit. */
export function isLinkKey(key: string): boolean {
  return key.startsWith("link:");
}

/** The project a link between two activities is in, as an edit request names it: the successor's. */
function projectOf(rows: Rows, successor: number): { project?: number } {
  const i = indexOf(rows, successor);
  const project = i === undefined ? undefined : rows.project[i];
  return project === undefined ? {} : { project };
}

function codeOf(rows: Rows, id: number): string {
  const i = indexOf(rows, id);
  return i === undefined ? String(id) : (rows.code[i] ?? String(id));
}

/**
 * A lag as typed, as the API takes it: a bare number in days where the
 * link's lag day is known and days are shown, else hours; empty is no lag.
 */
export function lagOf(text: string, perDay: number | null | undefined): string | CellProblem | null {
  if (text.trim() === "") return null;
  const value = span(text, unitsShown() === "days" && perDay ? "d" : "h");
  if (typeof value !== "string") return { message: `"${text.trim()}" isn't a lag`, hint: "type it as 2d, -1d or 4h" };
  return value;
}

/** A new link: `predecessor` to `successor` (by `task_id`), of `type`, with `lag` as the API takes it. */
export function addLink(rows: Rows, predecessor: number, successor: number, type: LinkType, lag: string | null): LinkChange {
  const [from, to] = [codeOf(rows, predecessor), codeOf(rows, successor)];
  return {
    key: addKey(predecessor, successor),
    edit: { op: "add_link", predecessor: from, successor: to, link_type: type, ...(lag ? { lag } : {}) },
    words: `${from} → ${to} added, ${type}${lag && Number.parseFloat(lag) !== 0 ? ` ${lag}` : ""}`,
    ...projectOf(rows, successor),
  };
}

/** A link already there, as an edit names it. */
export interface Linked {
  /** `task_pred_id`. */
  link: number;
  predecessor: number;
  successor: number;
  type: LinkType;
}

/** A span as the API takes it, in hours, where it can be told here: days need the lag's day. */
export function spanHours(text: string, perDay: number | null | undefined): number | null {
  const match = /^(-?[0-9]*\.?[0-9]+)([hd])$/.exec(text);
  if (!match) return null;
  const amount = Number(match[1]);
  if (match[2] === "h") return amount;
  return perDay ? amount * perDay : null;
}

/**
 * A change to a link's type, its lag, or both, joined with the change
 * already waiting for it, so the two make one `change_link`. A lag of null
 * is the link's own again. Null when nothing's left to change, as when the
 * type and lag go back to what they were.
 */
export function changeLink(
  rows: Rows,
  link: Linked,
  waiting: LinkChange | undefined,
  change: { type?: LinkType; lag?: string | null },
): LinkChange | null {
  const before = waiting?.edit.op === "change_link" ? waiting.edit : null;
  const type = change.type ?? before?.new_type ?? link.type;
  const lag = "lag" in change ? (change.lag ?? null) : (before?.lag ?? null);
  const [from, to] = [codeOf(rows, link.predecessor), codeOf(rows, link.successor)];
  const retyped = type !== link.type;
  if (!retyped && lag === null) return null;
  const parts = [retyped ? type : null, lag !== null ? `lag ${lag}` : null].filter(Boolean);
  return {
    key: linkKey(link.link),
    edit: {
      op: "change_link",
      predecessor: from,
      successor: to,
      link_type: link.type,
      ...(retyped ? { new_type: type } : {}),
      ...(lag !== null ? { lag } : {}),
    },
    words: `${from} → ${to}: ${parts.join(", ")}`,
    ...projectOf(rows, link.successor),
  };
}

/** Taking a link out. */
export function removeLink(rows: Rows, link: Linked): LinkChange {
  const [from, to] = [codeOf(rows, link.predecessor), codeOf(rows, link.successor)];
  return {
    key: linkKey(link.link),
    edit: { op: "remove_link", predecessor: from, successor: to, link_type: link.type },
    words: `${from} → ${to} removed`,
    ...projectOf(rows, link.successor),
  };
}
