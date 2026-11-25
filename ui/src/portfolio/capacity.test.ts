import { describe, expect, it } from "vitest";

import { golden } from "../api/schemas/golden";
import { amount, barsOf } from "./capacity";

type Line = (typeof golden)["portfolio-capacity-1"]["resources"][number];

function line(periods: [string, number, number | null, boolean][]): Line {
  const base = golden["portfolio-capacity-1"].resources[0];
  if (!base) throw new Error("the golden capacity has no resource");
  return { ...base, periods: periods.map(([start, demand, supply, over]) => ({ start, demand, supply, over })) };
}

describe("the capacity chart", () => {
  it("scales demand and supply together, the larger reaching the top", () => {
    const bars = barsOf(
      line([
        ["2026-01-05", 20, 40, false],
        ["2026-01-12", 80, 40, true],
      ]),
      200,
      100,
    );
    expect(bars.map((b) => [b.x, b.y, b.supplyY, b.over])).toEqual([
      [0, 75, 50, false],
      [100, 0, 50, true],
    ]);
  });

  it("leaves out the supply line where there's no limit", () => {
    const [bar] = barsOf(line([["2026-01-05", 10, null, false]]), 100, 100);
    expect(bar?.supplyY).toBeNull();
    expect(bar?.y).toBe(0);
  });

  it("writes amounts as a planner reads them", () => {
    expect(amount(12947.77)).toBe("12,948");
    expect(amount(2.25)).toBe("2.3");
  });
});
