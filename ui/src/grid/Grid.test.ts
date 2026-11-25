import { describe, expect, it } from "vitest";

import type { Rows } from "../api";
import { nextSort, rangeText } from "./Grid";

describe("clicking a header", () => {
  it("sorts ascending, then descending, then not", () => {
    const once = nextSort([], "code", false);
    expect(once).toEqual([{ column: "code", descending: false }]);
    const twice = nextSort(once, "code", false);
    expect(twice).toEqual([{ column: "code", descending: true }]);
    expect(nextSort(twice, "code", false)).toEqual([]);
  });

  it("adds a second key with Shift, keeping the first", () => {
    const first = nextSort([], "float", false);
    const both = nextSort(first, "code", true);
    expect(both).toEqual([
      { column: "float", descending: false },
      { column: "code", descending: false },
    ]);
    expect(nextSort(both, "code", true)[1]).toEqual({ column: "code", descending: true });
    // A plain click starts over with one key.
    expect(nextSort(both, "name", false)).toEqual([{ column: "name", descending: false }]);
  });

  it("adds a third key and more with Shift, after the others", () => {
    const two = nextSort(nextSort([], "wbs", false), "float", true);
    const three = nextSort(two, "code", true);
    expect(three).toEqual([
      { column: "wbs", descending: false },
      { column: "float", descending: false },
      { column: "code", descending: false },
    ]);
    // Shift on a key already there turns it, then takes it out, leaving the rest in order.
    expect(nextSort(three, "float", true)[1]).toEqual({ column: "float", descending: true });
    expect(nextSort(nextSort(three, "float", true), "float", true)).toEqual([
      { column: "wbs", descending: false },
      { column: "code", descending: false },
    ]);
  });
});

describe("copying a range", () => {
  const rows = {
    count: 3,
    code: ["A1", "A2", "A3"],
    name: ["Pour\tslab", "Cure", "Strip"],
    total_float_hours: [0, 8.5, null],
  } as unknown as Rows;

  it("gives tab-separated text of the activities in it, skipping bands", () => {
    const lines = Int32Array.from([-1, 0, 1, 2]);
    const text = rangeText(rows, lines, ["code", "name", "float"], { line: 3, column: 2 }, { line: 0, column: 0 });
    // No hours in a day for these rows, so float shows in hours, marked.
    expect(text).toBe("A1\tPour slab\t0h\nA2\tCure\t8.5h\nA3\tStrip\t");
  });
});
