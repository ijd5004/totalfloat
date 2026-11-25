// Scales for the Projects view's capacity chart: each period's
// demand as a bar against its supply as a line, over-allocated bars marked.

import type { CapacityReport } from "../api";

type Line = CapacityReport["resources"][number];

export interface Bar {
  start: string;
  demand: number;
  supply: number | null;
  over: boolean;
  x: number;
  width: number;
  /** Top of the demand bar; the bottom is the chart's height. */
  y: number;
  /** The supply's height, or null without a supply. */
  supplyY: number | null;
}

/** Bars scaled so the larger of the tallest demand and supply reaches the top. */
export function barsOf(line: Line, width: number, height: number): Bar[] {
  const periods = line.periods;
  const step = width / Math.max(periods.length, 1);
  const top = Math.max(...periods.map((p) => Math.max(p.demand, p.supply ?? 0)), 0);
  const scale = (v: number) => (top > 0 ? height - (v / top) * height : height);
  return periods.map((p, k) => ({
    start: p.start,
    demand: p.demand,
    supply: p.supply ?? null,
    over: p.over,
    x: k * step,
    width: Math.max(step - 1, 1),
    y: scale(p.demand),
    supplyY: p.supply == null ? null : scale(p.supply),
  }));
}

/** A number as a planner reads it: whole above 10, else to one place. */
export function amount(n: number): string {
  return Math.abs(n) >= 10 ? Math.round(n).toLocaleString("en-US") : n.toFixed(1);
}
