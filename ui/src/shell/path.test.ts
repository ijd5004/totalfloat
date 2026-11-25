import { describe, expect, it } from "vitest";

import type { Explanation } from "../api";
import { pathOf } from "./Shell";

describe("a driving path from explain", () => {
  it("takes the chain's activities, the relationships it names, and steps between activities naming none", () => {
    // A3's early start, back through A2 to A1; A1 to A3 also exists, but isn't on the chain.
    const report = {
      schema: "totalfloat.explain/1",
      chains: [
        {
          field: "early_start",
          date: "2026-01-14 08:00",
          origin: "data-date",
          total_steps: 4,
          steps: [
            // A3 driven by relationship 7 from A2; A2 by A1 through a step naming none.
            { activity: 3, code: "A3", date: null, text: "", relationship: 7 },
            { activity: 2, code: "A2", date: null, text: "" },
            { activity: 2, code: "A2", date: null, text: "" },
            { activity: 1, code: "A1", date: null, text: "" },
          ],
        },
      ],
    } as Explanation;
    const path = pathOf(report, 3);
    expect([...path.ids].sort()).toEqual([1, 2, 3]);
    expect([...path.relationships]).toEqual([7]);
    expect([...path.edges]).toEqual(["1>2"]);
  });
});
