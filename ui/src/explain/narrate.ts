// Puts an explanation in a scheduler's words. The engine's chain is a list of
// dates, each set by a relationship, a constraint, the data date or the
// activity's own duration. A scheduler reads it as a chain of activities:
// this one waits for that one, which waits for that one, back to where it
// starts. Each activity is one numbered stop, and the Gantt shows the same
// numbers on the same bars.

import type { Cause, Explanation, ExplainStep, Rows } from "../api";
import { dateText, datesShown } from "../dates";

/** One activity along the chain. */
export interface Stop {
  /** Its place along the chain, from 1, as the Gantt numbers it. */
  n: number;
  /** `task_id`. */
  id: number;
  code: string;
  name: string;
  /** What the chain says of it: its date, such as "Starts Thu 25 Jun 2026, 08:00". */
  date: string;
  /** Why, in plain words. */
  why: string;
}

export interface Narrative {
  /** The question answered, such as "Why does E1040 start on Thu 25 Jun 2026?" */
  question: string;
  stops: Stop[];
  /** The relationships the chain runs along, by `task_pred_id`, for the Gantt. */
  relationships: Set<number>;
}

/**
 * A date the server sent, in the format chosen for dates (`dates.ts`), such
 * as "25 Jun 26", with its time when times are shown and `withTime` allows
 * it; "no date" for none. No time zone is involved.
 */
export function readable(date: string | null | undefined, withTime = true): string {
  if (!date) return "no date";
  return dateText(date, withTime && datesShown().time);
}

/** A span in days, as people say it: "2 days", "half a day", "3.5 days". */
export function days(value: number): string {
  const rounded = Math.round(value * 100) / 100;
  if (rounded === 1) return "1 day";
  if (rounded === 0.5) return "half a day";
  return `${rounded} days`;
}

/** A lag as words, in days where the calendar's day is known, else hours. */
function lagWords(cause: Cause): string | null {
  const hours = cause.lag_hours ?? 0;
  if (hours === 0) return null;
  const amount = cause.lag_days != null ? days(Math.abs(cause.lag_days)) : `${Math.abs(Math.round(hours * 100) / 100)} hours`;
  return amount;
}

const CONSTRAINTS: Record<string, string> = {
  "start-on": "must start on",
  "start-on-or-after": "can't start before",
  "start-on-or-before": "must start by",
  "finish-on": "must finish on",
  "finish-on-or-after": "can't finish before",
  "finish-on-or-before": "must finish by",
  "mandatory-start": "must start exactly on",
  "mandatory-finish": "must finish exactly on",
  "as-late-as-possible": "is set to happen as late as possible",
};

/** Why a date is what it is, in a scheduler's words, naming the other activity by its ID and name. */
export function because(step: ExplainStep, other: (id: number) => string): string {
  const cause = step.cause;
  if (!cause) return step.text;
  // The other end is a successor for late dates and for dates pushed as late as possible.
  const backward = cause.from_role
    ? cause.from_role === "successor"
    : step.field === "late_start" || step.field === "late_finish";
  const alap = cause.as_late_as_possible ? "Set as late as possible: " : "";
  // Moving to working time is how every link and date lands on a calendar;
  // it isn't news to a scheduler, so it isn't said.
  const moved = "";
  switch (cause.kind) {
    case "relationship": {
      const them = other(cause.from ?? -1);
      const lag = lagWords(cause);
      const negative = (cause.lag_hours ?? 0) < 0;
      const gap = (event: string) =>
        lag ? `${lag} ${negative ? "before" : "after"} ${them} ${event}` : `${them} ${event}`;
      if (!backward) {
        switch (cause.link) {
          case "SS":
            return `It can't start until ${gap("starts")}${moved}.`;
          case "FF":
            return `It can't finish until ${gap("finishes")}${moved}.`;
          case "SF":
            return `It can't finish until ${gap("starts")}${moved}.`;
          default:
            return lag ? `It can't start until ${gap("finishes")}${moved}.` : `It can't start until ${them} finishes${moved}.`;
        }
      }
      // Toward a successor: how late it can be without holding that one up.
      // A positive lag must come before the successor's date; a negative one
      // lets it run past.
      const by = (event: string) =>
        !lag ? `before ${them} ${event}` : negative ? `no later than ${lag} after ${them} ${event}` : `${lag} before ${them} ${event}`;
      const verb = (what: string) => (negative && lag ? `can ${what}` : `must ${what}`);
      switch (cause.link) {
        case "SS":
          return `${alap}It ${verb("start")} ${by("starts")}${moved}.`;
        case "FF":
          return `${alap}It ${verb("finish")} ${by("finishes")}${moved}.`;
        case "SF":
          return `${alap}It ${verb("start")} ${by("finishes")}${moved}.`;
        default:
          return `${alap}It ${verb("finish")} ${by("starts")}${moved}.`;
      }
    }
    case "constraint":
      return `It ${CONSTRAINTS[cause.constraint ?? ""] ?? "has a constraint on"} ${readable(cause.date)}${moved}.`;
    case "data-date":
      return `Its work can't start before the data date${moved}.`;
    case "actual":
      return "That's when it actually happened.";
    case "resume":
      return "That's when it resumes after being suspended.";
    case "project-start":
      return `That's the project's planned start${moved}.`;
    case "open-end":
      return backward ? "Nothing follows it, so it can finish as late as the project does." : `Nothing comes before it, so it starts as soon as it can${moved}.`;
    case "project-finish":
      return "Nothing after it holds it sooner, so the project's finish sets it.";
    case "must-finish-by":
      return "The project must finish by then.";
    case "expected-finish":
      return "Its expected finish date sets it.";
    case "external-date":
      return `It waits on an activity in another project, on ${readable(cause.date)}.`;
    case "level-of-effort-span":
      return "As a level-of-effort activity, it spans the activities it's linked to.";
    case "wbs-summary-span":
      return "As a WBS summary activity, it spans the activities in its WBS.";
    default:
      return step.text;
  }
}

/** For each date: how a stop's date reads, and the question about it. */
const PHRASES: Record<string, { label: string; noun: string; ask: (code: string, date: string) => string }> = {
  early_start: { label: "Starts", noun: "start", ask: (c, d) => `Why does ${c} start on ${d}?` },
  early_finish: { label: "Finishes", noun: "finish", ask: (c, d) => `Why does ${c} finish on ${d}?` },
  late_start: { label: "Must start by", noun: "latest start", ask: (c, d) => `Why must ${c} start by ${d}?` },
  late_finish: { label: "Must finish by", noun: "latest finish", ask: (c, d) => `Why must ${c} finish by ${d}?` },
};

/**
 * One chain as numbered stops, one per activity. A stop's date and reason are
 * those of the first of its steps set by something other than its own
 * duration: what moved the activity, rather than "its start plus its length".
 */
export function narrate(report: Explanation, chain: number, rows: Rows, index: ReadonlyMap<number, number>): Narrative {
  const found = report.chains[chain];
  const relationships = new Set<number>();
  const nameOf = (id: number) => {
    const i = index.get(id);
    return i === undefined ? { code: String(id), name: "" } : { code: rows.code[i] ?? String(id), name: rows.name[i] ?? "" };
  };
  const other = (id: number) => {
    const { code, name } = nameOf(id);
    return name ? `${code} ${name}` : code;
  };
  if (!found) return { question: "", stops: [], relationships };

  const groups: ExplainStep[][] = [];
  for (const step of found.steps) {
    const last = groups[groups.length - 1];
    if (last && last[0]?.activity === step.activity) last.push(step);
    else groups.push([step]);
    if (step.relationship != null) relationships.add(step.relationship);
  }
  const stops = groups.map((steps, k): Stop => {
    const telling = (steps.find((step) => step.cause?.kind !== "duration") ?? steps[steps.length - 1] ?? steps[0]) as ExplainStep;
    const { code, name } = nameOf(telling.activity);
    const label = PHRASES[telling.field ?? ""]?.label ?? "Date";
    let why = because(telling, other);
    // The date asked about follows from another of its dates and its duration.
    const asked = steps[0];
    if (k === 0 && asked && asked !== telling && asked.cause?.kind === "duration") {
      why += ` Its ${PHRASES[asked.field ?? ""]?.noun ?? "date"} follows from that and its duration.`;
    }
    return { n: k + 1, id: telling.activity, code, name, date: `${label} ${readable(telling.date)}`, why };
  });

  const first = found.steps[0];
  const subject = first ? nameOf(first.activity).code : (report.activity?.code ?? "");
  const ask = PHRASES[found.field]?.ask ?? ((c: string, d: string) => `Why is ${c}'s date ${d}?`);
  const question = ask(subject, readable(found.date, false));
  return { question, stops, relationships };
}
