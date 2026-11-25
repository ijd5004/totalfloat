// How dates are shown: one format, chosen by the person and kept in the
// browser, for the grid, the Gantt's tooltips, the panels and print. The
// server sends civil dates as `YYYY-MM-DD HH:MM` (ARCHITECTURE.md, Data model); this only
// rewrites that text, with no time zone and no calendar involved. What can be
// typed into a cell doesn't change (`edit/cells.ts`).

/** A way of writing the day. */
export type DateStyle = "dd-mmm-yy" | "dd mmm yy" | "yyyy-mm-dd" | "dd/mm/yyyy" | "mm/dd/yyyy";

export interface DateFormat {
  style: DateStyle;
  /** Whether a time follows the day. */
  time: boolean;
}

/** Each style, with 31 May 2026 written in it, for menus. */
export const DATE_STYLES: readonly { style: DateStyle; example: string }[] = [
  { style: "dd mmm yy", example: "31 May 26" },
  { style: "dd-mmm-yy", example: "31-May-26" },
  { style: "yyyy-mm-dd", example: "2026-05-31" },
  { style: "dd/mm/yyyy", example: "31/05/2026" },
  { style: "mm/dd/yyyy", example: "05/31/2026" },
];

export const DEFAULT_DATES: DateFormat = { style: "dd mmm yy", time: false };

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

let shown: DateFormat = DEFAULT_DATES;

/** The format dates are shown in now. */
export function datesShown(): DateFormat {
  return shown;
}

/** Shows dates in `format` from now on. */
export function showDates(format: DateFormat): void {
  shown = format;
}

/** Whether `value` is a style this app knows. */
export function isDateStyle(value: unknown): value is DateStyle {
  return DATE_STYLES.some((known) => known.style === value);
}

const DATE = /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2}))?/;

/**
 * A date the server sent, in the format shown: `05 May 26`, with `08:00`
 * after it when times are shown and the date has one. `time` overrides the
 * setting, for places that only ever name the day. Empty for none; text that
 * isn't a date comes back as it was.
 */
export function dateText(text: string | null | undefined, time: boolean = shown.time): string {
  if (!text) return "";
  const match = DATE.exec(text);
  if (!match) return text;
  const [, y = "", mo = "", d = "", h, mi] = match;
  const month = MONTHS[Number(mo) - 1] ?? mo;
  let day: string;
  switch (shown.style) {
    case "dd mmm yy":
      day = `${d} ${month} ${y.slice(2)}`;
      break;
    case "dd-mmm-yy":
      day = `${d}-${month}-${y.slice(2)}`;
      break;
    case "yyyy-mm-dd":
      day = `${y}-${mo}-${d}`;
      break;
    case "dd/mm/yyyy":
      day = `${d}/${mo}/${y}`;
      break;
    case "mm/dd/yyyy":
      day = `${mo}/${d}/${y}`;
      break;
  }
  return time && h !== undefined && mi !== undefined ? `${day} ${h}:${mi}` : day;
}

/**
 * Two dates of a move, from and to, in the format shown. When times are
 * hidden and both fall on the same day, they carry their times, so a move
 * within a day doesn't read as no move.
 */
export function moveText(from: string | null | undefined, to: string | null | undefined): [string, string] {
  const sameDay = !!from && !!to && from !== to && from.slice(0, 10) === to.slice(0, 10);
  return [dateText(from, sameDay || shown.time), dateText(to, sameDay || shown.time)];
}
