// Scales for the risk view's charts: the finish histogram with its
// cumulative curve, and the tornado of sensitivities. Pure functions, so
// the drawing is a plain map from these numbers.

import type { RiskReport } from "../api";

/** One day's bar and the cumulative curve's point at its end, in pixels. */
export interface DayBar {
  date: string;
  /** The share of iterations finishing that day. */
  share: number;
  /** The share finished by its end. */
  cumulative: number;
  x: number;
  width: number;
  /** Top of the bar; the bottom is the chart's height. */
  y: number;
  /** The curve's height at the end of the day. */
  curveY: number;
}

export interface Histogram {
  bars: DayBar[];
  /** The curve through each day's end, as an SVG path. */
  curve: string;
  /** The tallest bar's share, which the bar scale reaches at the top. */
  peak: number;
}

/**
 * Bars for each day from the earliest finish to the latest, scaled so the
 * tallest reaches the top, and the cumulative curve from 0 at the left to 1
 * at the top. `width` and `height` are the plot's, in pixels.
 */
export function histogramOf(report: RiskReport, width: number, height: number): Histogram {
  const days = report.cumulative;
  const count = Math.max(days.length, 1);
  const step = width / count;
  let before = 0;
  const shares = days.map((day) => {
    const share = Math.max(0, day.probability - before);
    before = day.probability;
    return share;
  });
  const peak = Math.max(...shares, 0);
  const bars = days.map((day, k) => {
    const share = shares[k] ?? 0;
    return {
      date: day.date,
      share,
      cumulative: day.probability,
      x: k * step,
      width: Math.max(step - 1, 1),
      y: peak > 0 ? height - (share / peak) * height : height,
      curveY: height - day.probability * height,
    };
  });
  const curve = bars.length
    ? `M0 ${height} ` + bars.map((b) => `L${round(b.x + step)} ${round(b.curveY)}`).join(" ")
    : "";
  return { bars, curve, peak };
}

/**
 * Where a finish falls on the histogram's x axis: the end of its day's bar
 * plus its time of day, or null if it's outside the days shown.
 */
export function xOf(report: RiskReport, finish: string, width: number): number | null {
  const days = report.cumulative;
  const step = width / Math.max(days.length, 1);
  // A finish at midnight ends the day before, as the report counts it.
  const [date, time] = finish.split(" ");
  const k = days.findIndex((day) => day.date === date);
  if (time === "00:00" && date) {
    const before = days.findIndex((day) => day.date === dayBefore(date));
    return before >= 0 ? (before + 1) * step : null;
  }
  if (k < 0) return null;
  const [h, m] = (time ?? "00:00").split(":").map(Number);
  return k * step + (((h ?? 0) * 60 + (m ?? 0)) / 1440) * step;
}

/** One bar of the tornado: a sensitivity from -1 to 1 around the middle. */
export interface TornadoBar {
  code: string;
  name: string;
  value: number;
  /** Left edge and width, in pixels; the middle is `width / 2`. */
  x: number;
  width: number;
}

export function tornadoOf(report: RiskReport, width: number): TornadoBar[] {
  const middle = width / 2;
  return report.sensitivity.map((r) => {
    const length = Math.abs(r.value) * middle;
    return {
      code: r.code,
      name: r.name,
      value: r.value,
      x: r.value >= 0 ? middle : middle - length,
      width: length,
    };
  });
}

/** The civil day before `date`, as YYYY-MM-DD. */
function dayBefore(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  const at = new Date(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1) - 86_400_000);
  return at.toISOString().slice(0, 10);
}

function round(n: number): number {
  return Math.round(n * 10) / 10;
}

/** A share as a whole percent. */
export function percent(share: number): string {
  return `${Math.round(share * 100)}%`;
}
