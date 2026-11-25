// The finish and float the title block shows for the first project, and how
// the edits waiting move them. Nothing is calculated here: the float is the
// lowest total float the server gave an unfinished activity, and with edits
// waiting, the preview's finish and the floats it changed.

import type { EditReport, Rows, Summary } from "../api";
import { minutesOf } from "../gantt/time";
import { spanText, unitsShown } from "../grid/columns";
import type { Outlook } from "./Bars";

/** A float the server writes, `16h` or `-8h`, in hours. */
function hoursOf(text: string | null | undefined): number | null {
  const found = /^(-?\d+(?:\.\d+)?)h$/.exec(text ?? "");
  return found ? Number(found[1]) : null;
}

/** How far a finish moved, in calendar days, or hours within a day: `+4d`, `−8h`. */
export function movedText(from: string | null | undefined, to: string | null | undefined): string | null {
  const minutes = minutesOf(to) - minutesOf(from);
  if (!Number.isFinite(minutes) || minutes === 0) return null;
  const sign = minutes > 0 ? "+" : "−";
  const size = Math.abs(minutes);
  return size < 24 * 60 ? `${sign}${Math.round(size / 6) / 10}h` : `${sign}${Math.round(size / (24 * 60))}d`;
}

/** The lowest total float among a project's unfinished activities, with `changed` floats, by code, in place of the rows'. */
function lowest(rows: Rows, project: number, changed?: ReadonlyMap<string, number | null>): { text: string; hours: number } | null {
  let best: number | null = null;
  let at = -1;
  for (let i = 0; i < rows.count; i++) {
    if (rows.project[i] !== project || rows.status[i] === "completed") continue;
    const code = rows.code[i] ?? "";
    const hours = changed?.has(code) ? (changed.get(code) ?? null) : (rows.total_float_hours[i] ?? null);
    if (hours === null) continue;
    if (best === null || hours < best) {
      best = hours;
      at = i;
    }
  }
  if (best === null) return null;
  // As the grid shows floats, days of that activity's calendar or hours, with
  // the unit the grid's title carries, and a minus sign rather than a hyphen.
  const text = spanText(best, rows.hours_per_day?.[at]);
  const unit = text.endsWith("h") ? "" : unitsShown() === "hours" ? "h" : "d";
  return { text: `${text.replace(/^-/, "−")}${unit}`, hours: best };
}

/** Edits that add, delete or progress activities, which change what counts as unfinished work. */
const RESHAPING = new Set(["add_activity", "delete_activity", "set_progress"]);

export function outlookOf(
  rows: Rows | null,
  summary: Summary | null,
  preview: EditReport | null,
  edits: readonly { op: string }[] = [],
): Outlook {
  const project = summary?.inspect.projects[0];
  if (!rows || !project) return { float: null };
  const now = lowest(rows, project.id);
  const float = now?.text ?? null;
  const impact = preview?.projects.find((p) => p.id === project.id);
  if (!impact) return { float };
  // The float the edits leave is known here only when the preview lists every
  // changed float (it lists up to a limit) and no activity is added, deleted
  // or progressed. Otherwise no float is shown.
  const complete =
    impact.changes.activities.length >= impact.changed_dates_or_float &&
    impact.added.length === 0 &&
    impact.deleted.length === 0 &&
    !edits.some((edit) => RESHAPING.has(edit.op));
  const changed = new Map<string, number | null>();
  for (const activity of impact.changes.activities) {
    const field = activity.fields.find((f) => f.field === "total_float");
    if (field) changed.set(activity.code, hoursOf(field.to));
  }
  const after = complete ? lowest(rows, project.id, changed) : null;
  return {
    float,
    pending: {
      finish: impact.scheduled_finish,
      moved: movedText(impact.previous_finish, impact.scheduled_finish),
      float: after?.text ?? null,
      lower: after !== null && now !== null && after.hours < now.hours,
    },
  };
}
