// Time on the Gantt's axis. Dates are the server's `YYYY-MM-DD HH:MM`, civil
// and time-zone free (ARCHITECTURE.md, Data model); each maps linearly to the screen, as minutes
// since 1970 read as if UTC. Nothing here works out a schedule date: no
// calendar is consulted, only where a date the server sent falls on screen.

import type { Rows } from "../api";

const MINUTES_PER_DAY = 24 * 60;

/** A date the server sent, in minutes since 1970, or NaN for none. */
export function minutesOf(text: string | null | undefined): number {
  if (!text || text.length < 16) return Number.NaN;
  const y = Number(text.slice(0, 4));
  const mo = Number(text.slice(5, 7));
  const d = Number(text.slice(8, 10));
  const h = Number(text.slice(11, 13));
  const mi = Number(text.slice(14, 16));
  const ms = Date.UTC(y, mo - 1, d, h, mi);
  return Number.isFinite(ms) ? ms / 60_000 : Number.NaN;
}

/** Where a date falls on screen. */
export interface Scale {
  /** The minute at x = 0, before scrolling. */
  origin: number;
  pxPerMinute: number;
}

export function xOf(scale: Scale, minutes: number): number {
  return (minutes - scale.origin) * scale.pxPerMinute;
}

export function minutesAt(scale: Scale, x: number): number {
  return scale.origin + x / scale.pxPerMinute;
}

export type Unit = "year" | "quarter" | "month" | "week" | "day";

/** One zoom level: how wide a day is, and the units of the two tiers. */
export interface Zoom {
  pxPerDay: number;
  top: Unit;
  bottom: Unit;
}

/** From farthest out to nearest in. */
export const ZOOMS: readonly Zoom[] = [
  { pxPerDay: 0.25, top: "year", bottom: "quarter" },
  { pxPerDay: 1, top: "year", bottom: "month" },
  { pxPerDay: 3, top: "quarter", bottom: "month" },
  { pxPerDay: 8, top: "month", bottom: "week" },
  { pxPerDay: 24, top: "month", bottom: "day" },
  { pxPerDay: 60, top: "week", bottom: "day" },
];

/** A zoom chosen: a fixed level, by its index in `ZOOMS`, or `fit`, which fits the dates shown into the chart. */
export type ZoomChoice = number | "fit";

/** The Gantt opens with the whole project in view. */
export const DEFAULT_ZOOM: ZoomChoice = "fit";

export function scaleOf(origin: number, zoom: Zoom): Scale {
  return { origin, pxPerMinute: zoom.pxPerDay / MINUTES_PER_DAY };
}

/** The fixed level nearest `pxPerDay`, by ratio. */
export function nearestLevel(pxPerDay: number): number {
  let best = 0;
  ZOOMS.forEach((zoom, k) => {
    const gap = Math.abs(Math.log(zoom.pxPerDay / pxPerDay));
    if (gap < Math.abs(Math.log((ZOOMS[best]?.pxPerDay ?? 1) / pxPerDay))) best = k;
  });
  return best;
}

/** The scale `fit` gives, and the minute to put at the chart's left edge. */
export interface Fitted {
  zoom: Zoom;
  left: number;
}

/**
 * The zoom that fits the minutes from `first` to `last` into `width` pixels,
 * with a margin each side of 3% of the span, at least a day; the tiers are
 * the nearest fixed level's. Null when there's nothing to fit.
 */
export function fitZoom(first: number, last: number, width: number): Fitted | null {
  if (!Number.isFinite(first) || !Number.isFinite(last) || last < first || width <= 0) return null;
  const margin = Math.max((last - first) * 0.03, MINUTES_PER_DAY);
  const days = (last - first + 2 * margin) / MINUTES_PER_DAY;
  const pxPerDay = Math.min(10_000, Math.max(0.01, width / days));
  const tiers = ZOOMS[nearestLevel(pxPerDay)] ?? ZOOMS[0];
  return { zoom: { pxPerDay, top: tiers?.top ?? "year", bottom: tiers?.bottom ?? "month" }, left: first - margin };
}

/**
 * The fixed level one step in (`by` 1) or out (-1) from `now`. From `fit`,
 * at `fitted` pixels a day, it's the nearest level past the fitted scale in
 * that direction; with none past it, it stays at `fit`.
 */
export function stepZoom(now: ZoomChoice, fitted: number, by: 1 | -1): ZoomChoice {
  if (now !== "fit") return Math.max(0, Math.min(ZOOMS.length - 1, now + by));
  let next: ZoomChoice = "fit";
  ZOOMS.forEach((zoom, k) => {
    if (by > 0 ? zoom.pxPerDay > fitted && next === "fit" : zoom.pxPerDay < fitted) next = k;
  });
  return next;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** The start of the unit holding `minutes`. Weeks start on Monday. */
export function unitStart(minutes: number, unit: Unit): number {
  const at = new Date(minutes * 60_000);
  const y = at.getUTCFullYear();
  const mo = at.getUTCMonth();
  const d = at.getUTCDate();
  switch (unit) {
    case "year":
      return Date.UTC(y, 0, 1) / 60_000;
    case "quarter":
      return Date.UTC(y, mo - (mo % 3), 1) / 60_000;
    case "month":
      return Date.UTC(y, mo, 1) / 60_000;
    case "week":
      return Date.UTC(y, mo, d - ((at.getUTCDay() + 6) % 7)) / 60_000;
    case "day":
      return Date.UTC(y, mo, d) / 60_000;
  }
}

/** The start of the unit after the one starting at `start`. */
export function nextUnit(start: number, unit: Unit): number {
  const at = new Date(start * 60_000);
  const y = at.getUTCFullYear();
  const mo = at.getUTCMonth();
  const d = at.getUTCDate();
  switch (unit) {
    case "year":
      return Date.UTC(y + 1, 0, 1) / 60_000;
    case "quarter":
      return Date.UTC(y, mo + 3, 1) / 60_000;
    case "month":
      return Date.UTC(y, mo + 1, 1) / 60_000;
    case "week":
      return Date.UTC(y, mo, d + 7) / 60_000;
    case "day":
      return Date.UTC(y, mo, d + 1) / 60_000;
  }
}

/** A unit's label: fuller on the top tier, which has the room. */
export function labelOf(start: number, unit: Unit, tier: "top" | "bottom"): string {
  const at = new Date(start * 60_000);
  const y = at.getUTCFullYear();
  const mo = at.getUTCMonth();
  const d = at.getUTCDate();
  switch (unit) {
    case "year":
      return String(y);
    case "quarter":
      return tier === "top" ? `Q${Math.floor(mo / 3) + 1} ${y}` : `Q${Math.floor(mo / 3) + 1}`;
    case "month":
      return tier === "top" ? `${MONTHS[mo]} ${y}` : (MONTHS[mo] ?? "");
    case "week":
      return tier === "top" ? `${d} ${MONTHS[mo]} ${y}` : String(d);
    case "day":
      return tier === "top" ? `${DAYS[at.getUTCDay()]} ${d} ${MONTHS[mo]}` : `${DAYS[at.getUTCDay()]?.[0] ?? ""} ${d}`;
  }
}

/** A cell of a tier: where it starts on screen, how wide it is, and its label. */
export interface Tick {
  x: number;
  width: number;
  label: string;
}

/** The cells of one tier between screen x `from` and `to`. */
export function ticks(scale: Scale, unit: Unit, tier: "top" | "bottom", from: number, to: number): Tick[] {
  const out: Tick[] = [];
  let start = unitStart(minutesAt(scale, from), unit);
  // Far more than a screen holds at any zoom; a guard, not a limit anyone meets.
  for (let n = 0; n < 2000; n++) {
    const x = xOf(scale, start);
    if (x > to) break;
    const next = nextUnit(start, unit);
    out.push({ x, width: xOf(scale, next) - x, label: labelOf(start, unit, tier) });
    start = next;
  }
  return out;
}

/** Each activity's dates as minutes, read once per set of rows. NaN is none. */
export interface Times {
  earlyStart: Float64Array;
  earlyFinish: Float64Array;
  lateFinish: Float64Array;
  actualStart: Float64Array;
  actualFinish: Float64Array;
  /** The first and last minute any of them holds. */
  first: number;
  last: number;
}

const cache = new WeakMap<Rows, Times>();

export function timesOf(rows: Rows): Times {
  const known = cache.get(rows);
  if (known) return known;
  const read = (column: (string | null)[]) => {
    const out = new Float64Array(rows.count);
    for (let i = 0; i < rows.count; i++) out[i] = minutesOf(column[i]);
    return out;
  };
  const times: Times = {
    earlyStart: read(rows.early_start),
    earlyFinish: read(rows.early_finish),
    lateFinish: read(rows.late_finish),
    actualStart: read(rows.actual_start),
    actualFinish: read(rows.actual_finish),
    first: Number.POSITIVE_INFINITY,
    last: Number.NEGATIVE_INFINITY,
  };
  for (const column of [times.earlyStart, times.actualStart]) {
    for (const value of column) if (value < times.first) times.first = value;
  }
  for (const column of [times.earlyFinish, times.lateFinish, times.actualFinish]) {
    for (const value of column) if (value > times.last) times.last = value;
  }
  cache.set(rows, times);
  return times;
}
