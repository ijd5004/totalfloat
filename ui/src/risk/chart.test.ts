import { describe, expect, it } from "vitest";

import { golden } from "../api/schemas/golden";
import type { RiskReport } from "../api";
import { histogramOf, percent, tornadoOf, xOf } from "./chart";

function report(cumulative: [string, number][], sensitivity: [string, number][] = []): RiskReport {
  return {
    ...golden["risk-analysis-1"],
    cumulative: cumulative.map(([date, probability]) => ({ date, probability })),
    sensitivity: sensitivity.map(([code, value]) => ({ code, name: code, value })),
  };
}

describe("the finish histogram", () => {
  it("gives each day its share, the tallest reaching the top, and the curve to 1", () => {
    // 20% finish on the 1st, 60% on the 2nd, 20% on the 3rd.
    const h = histogramOf(report([["2026-01-01", 0.2], ["2026-01-02", 0.8], ["2026-01-03", 1]]), 300, 100);
    expect(h.bars.map((b) => Math.round(b.share * 100))).toEqual([20, 60, 20]);
    expect(h.peak).toBeCloseTo(0.6);
    expect(h.bars.map((b) => b.x)).toEqual([0, 100, 200]);
    expect(h.bars[1]?.y).toBe(0);
    expect(h.bars[0]?.y).toBeCloseTo(100 - (0.2 / 0.6) * 100);
    expect(h.bars.map((b) => b.curveY)).toEqual([80, 20, 0]);
    expect(h.curve).toBe("M0 100 L100 80 L200 20 L300 0");
  });

  it("puts a finish at its time of day, and midnight at the end of the day before", () => {
    const r = report([["2026-01-01", 0.5], ["2026-01-02", 1]]);
    expect(xOf(r, "2026-01-02 12:00", 200)).toBe(150);
    expect(xOf(r, "2026-01-02 00:00", 200)).toBe(100);
    expect(xOf(r, "2026-01-09 08:00", 200)).toBeNull();
    // Midnight after the last day is the plot's right edge.
    expect(xOf(r, "2026-01-03 00:00", 200)).toBe(200);
    expect(xOf(r, "2026-01-01 00:00", 200)).toBeNull();
  });
});

describe("the tornado", () => {
  it("runs positive bars right of the middle and negative ones left", () => {
    const bars = tornadoOf(report([], [["A1", 0.5], ["A2", -0.25]]), 200);
    expect(bars.map((b) => [b.code, b.x, b.width])).toEqual([
      ["A1", 100, 50],
      ["A2", 75, 25],
    ]);
  });

  it("shows shares as whole percents", () => {
    expect(percent(0.256)).toBe("26%");
  });
});
