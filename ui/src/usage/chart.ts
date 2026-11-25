// The usage histogram's layout: each period's actual and remaining
// units stacked as bars, and the resource's limit as a line stepping from
// period to period. Pure functions, so drawing is a plain map from these
// numbers; the units themselves are the server's (`/api/1/usage`).

import type { UsagePeriod } from "../api";

/** One period's bar, in pixels; the plot's bottom is `height`. */
export interface UsageBar {
  start: string;
  actual: number;
  remaining: number;
  limit: number | null;
  /** More than the limit, as the server says. */
  over: boolean;
  x: number;
  width: number;
  /** Top of the actual part; it runs down to the bottom. */
  actualY: number;
  /** Top of the remaining part, which sits on the actual part. */
  remainingY: number;
  /** The limit's height in this period, or null without one. */
  limitY: number | null;
}

export interface UsageChart {
  bars: UsageBar[];
  /** The limit as an SVG path, a level step across each period that has one,
   * joined to the next period's step, and broken where a period has none. */
  limitPath: string;
  /** What the top of the plot stands for: the tallest bar or limit. */
  top: number;
}

const round = (n: number) => Math.round(n * 10) / 10;

/**
 * Bars for the periods, left to right, scaled so the tallest bar or limit
 * reaches the top. `width` and `height` are the plot's, in pixels.
 */
export function usageChart(periods: readonly UsagePeriod[], width: number, height: number): UsageChart {
  const top = Math.max(0, ...periods.map((p) => Math.max(p.total, p.limit ?? 0)));
  const step = width / Math.max(periods.length, 1);
  const y = (units: number) => (top > 0 ? height - (units / top) * height : height);
  const bars = periods.map((p, k) => ({
    start: p.start,
    actual: p.actual,
    remaining: p.remaining,
    limit: p.limit ?? null,
    over: p.over,
    x: round(k * step),
    width: round(Math.max(step - 2, 1)),
    actualY: round(y(p.actual)),
    remainingY: round(y(p.actual + p.remaining)),
    limitY: p.limit === null || p.limit === undefined ? null : round(y(p.limit)),
  }));
  const limitPath = bars
    .map((b, k) => {
      if (b.limitY === null) return "";
      const before = bars[k - 1];
      const joined = before !== undefined && before.limitY !== null;
      return `${joined ? `V${b.limitY}` : `M${b.x} ${b.limitY}`} H${round(b.x + step)}`;
    })
    .filter((part) => part !== "")
    .join(" ");
  return { bars, limitPath, top };
}

/** Units as the table shows them: up to 2 places, no trailing zeros. */
export function units(n: number): string {
  return (Math.round(n * 100) / 100).toLocaleString("en-US", { maximumFractionDigits: 2 });
}
