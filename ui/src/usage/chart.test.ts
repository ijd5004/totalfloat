import { describe, expect, it } from "vitest";

import type { UsagePeriod } from "../api";
import { units, usageChart } from "./chart";

const period = (start: string, actual: number, remaining: number, limit: number | null): UsagePeriod => ({
  start,
  actual,
  remaining,
  total: actual + remaining,
  limit,
  over: limit !== null && actual + remaining > limit,
});

describe("the usage histogram's layout", () => {
  const periods = [period("2026-01-05", 20, 10, 40), period("2026-01-12", 0, 60, 40), period("2026-01-19", 0, 10, null)];

  it("stacks remaining on actual, scaled so the tallest bar or limit reaches the top", () => {
    const chart = usageChart(periods, 300, 100);
    expect(chart.top).toBe(60);
    const [first, second, third] = chart.bars;
    expect(first).toMatchObject({ x: 0, width: 98, actualY: 66.7, remainingY: 50, limitY: 33.3, over: false });
    expect(second).toMatchObject({ x: 100, actualY: 100, remainingY: 0, limitY: 33.3, over: true });
    expect(third).toMatchObject({ x: 200, remainingY: 83.3, limitY: null, over: false });
  });

  it("draws the limit as a level step in each period that has one, joined from step to step", () => {
    expect(usageChart(periods, 300, 100).limitPath).toBe("M0 33.3 H100 V33.3 H200");
    const stepped = [period("2026-01-05", 0, 10, 40), period("2026-01-12", 0, 10, 20), period("2026-01-19", 0, 10, null), period("2026-01-26", 0, 10, 40)];
    expect(usageChart(stepped, 400, 100).limitPath).toBe("M0 0 H100 V50 H200 M300 0 H400");
  });

  it("marks a period over its limit as the server says, and none without a limit", () => {
    const marks = usageChart(periods, 300, 100).bars.map((b) => b.over);
    expect(marks).toEqual([false, true, false]);
  });

  it("puts a limit above every bar at the top", () => {
    const chart = usageChart([period("2026-01-05", 5, 5, 80)], 100, 100);
    expect(chart.top).toBe(80);
    expect(chart.bars[0]?.limitY).toBe(0);
    expect(chart.bars[0]?.remainingY).toBe(87.5);
  });

  it("lays out nothing for no periods, and a flat chart for no units", () => {
    expect(usageChart([], 300, 100)).toEqual({ bars: [], limitPath: "", top: 0 });
    const flat = usageChart([period("2026-01-05", 0, 0, null)], 300, 100);
    expect(flat.bars[0]?.remainingY).toBe(100);
  });

  it("shows units to two places", () => {
    expect(units(1234.567)).toBe("1,234.57");
    expect(units(8)).toBe("8");
  });
});
