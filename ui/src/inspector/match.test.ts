import { describe, expect, it } from "vitest";

import { sample } from "../edit/fixture";
import { matches, split } from "./match";

const rows = sample({
  id: [1, 2, 3, 4, 5],
  code: ["A1", "A10", "B1", "XA1", "C9"],
  name: ["Excavate", "Pour slab", "Glazing L1", "Glazing L2", "A1 rework"],
  project: [7, 7, 7, 7, 7],
});
(rows as { count: number }).count = 5;

describe("matching what's typed in Add predecessor", () => {
  it("ranks an exact ID, then IDs starting with it, IDs holding it, names starting with it, names holding it", () => {
    expect(matches(rows, "a1", -1).map((i) => rows.code[i])).toEqual(["A1", "A10", "XA1", "C9"]);
  });

  it("matches part of a name, ignoring case, and never the activity itself", () => {
    expect(matches(rows, "GLAZ", -1).map((i) => rows.code[i])).toEqual(["B1", "XA1"]);
    expect(matches(rows, "glaz", 2).map((i) => rows.code[i])).toEqual(["XA1"]);
  });

  it("lists nothing for nothing typed, and at most the limit", () => {
    expect(matches(rows, "  ", -1)).toEqual([]);
    expect(matches(rows, "a", -1, 2)).toHaveLength(2);
  });

  it("splits a name where the typed text falls, for showing it bold", () => {
    expect(split("Glazing L3", "glaz")).toEqual(["", "Glaz", "ing L3"]);
    expect(split("Glazing L3", "x")).toBeNull();
  });
});
