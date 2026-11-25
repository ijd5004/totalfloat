import { describe, expect, it } from "vitest";

import type { Links } from "../api";
import { STUB, distanceTo, edgesOf, joins, lineIndex, linksByActivity, route } from "./lines";

const ROW = 22;
// A predecessor on row 1 and a successor two rows down.
const pred = { left: 100, right: 200, y: 33 };
const after = { left: 260, right: 360, y: 77 };
const overlapping = { left: 150, right: 180, y: 77 };

describe("logic lines", () => {
  it("join the ends each type names", () => {
    expect(joins("FS")).toEqual({ from: "finish", to: "start" });
    expect(joins("SS")).toEqual({ from: "start", to: "start" });
    expect(joins("FF")).toEqual({ from: "finish", to: "finish" });
    expect(joins("SF")).toEqual({ from: "start", to: "finish" });
  });

  it("route FS out of the finish and into the start from the left", () => {
    expect(route("FS", pred, after, ROW)).toEqual([
      [200, 33],
      [200 + STUB, 33],
      [200 + STUB, 77],
      [260, 77],
    ]);
  });

  it("dogleg an FS whose successor starts before the predecessor finishes", () => {
    expect(route("FS", pred, overlapping, ROW)).toEqual([
      [200, 33],
      [206, 33],
      [206, 44],
      [144, 44],
      [144, 77],
      [150, 77],
    ]);
  });

  it("route SS out of the left of the start and into the successor's start", () => {
    expect(route("SS", pred, after, ROW)).toEqual([
      [100, 33],
      [94, 33],
      [94, 77],
      [260, 77],
    ]);
  });

  it("route FF out of the finish and into the successor's finish from the right", () => {
    // The successor finishes later, so the line comes back to it from its right.
    expect(route("FF", pred, after, ROW)).toEqual([
      [200, 33],
      [206, 33],
      [206, 44],
      [366, 44],
      [366, 77],
      [360, 77],
    ]);
    // A successor finishing earlier is reached directly.
    expect(route("FF", pred, overlapping, ROW)).toEqual([
      [200, 33],
      [206, 33],
      [206, 77],
      [180, 77],
    ]);
  });

  it("route SF out of the start and into the successor's finish", () => {
    expect(route("SF", pred, { left: 20, right: 60, y: 77 }, ROW)).toEqual([
      [100, 33],
      [94, 33],
      [94, 77],
      [60, 77],
    ]);
  });

  it("go up when the successor is above", () => {
    const above = { left: 150, right: 180, y: 11 };
    expect(route("FS", { left: 100, right: 200, y: 55 }, above, ROW)[2]).toEqual([206, 44]);
  });

  it("put an arrow where a line leaves the screen", () => {
    // The successor is far below a 300-pixel-high chart.
    const points = route("FS", pred, { left: 260, right: 360, y: 1000 }, ROW);
    expect(edgesOf(points, 300)).toEqual([{ x: 206, y: 300, edge: "bottom", toward: "to" }]);
    // The predecessor is above the screen.
    const fromAbove = route("FS", { left: 100, right: 200, y: -500 }, after, ROW);
    expect(edgesOf(fromAbove, 300)).toEqual([{ x: 206, y: 0, edge: "top", toward: "from" }]);
    // Both ends off screen, one above and one below: an arrow at each edge.
    const across = route("FS", { left: 100, right: 200, y: -500 }, { left: 260, right: 360, y: 1000 }, ROW);
    expect(edgesOf(across, 300)).toEqual([
      { x: 206, y: 0, edge: "top", toward: "from" },
      { x: 206, y: 300, edge: "bottom", toward: "to" },
    ]);
    // A dogleg that runs along the top edge still crosses it.
    const along = route("FS", { left: 30, right: 63, y: -11 }, { left: 30, right: 63, y: 11 }, ROW);
    expect(edgesOf(along, 300)).toEqual([{ x: 69, y: 0, edge: "top", toward: "from" }]);
    // A successor beyond the right edge, on a row on screen.
    const right = route("FS", pred, { left: 900, right: 950, y: 77 }, ROW);
    expect(edgesOf(right, 300, 600)).toEqual([{ x: 600, y: 77, edge: "right", toward: "to" }]);
    // Both ends beyond the right edge, the route coming back on screen between
    // them: each crossing leads to the end on its own side.
    const out = route("SS", { left: 604, right: 640, y: 33 }, { left: 612, right: 650, y: 77 }, ROW);
    expect(edgesOf(out, 300, 600).map((c) => [c.edge, c.toward])).toEqual([
      ["right", "from"],
      ["right", "to"],
    ]);
    // Both ends on screen: no arrow.
    expect(edgesOf(route("FS", pred, after, ROW), 300)).toEqual([]);
  });

  it("measure the distance to a line, for hovering", () => {
    const points = route("FS", pred, after, ROW);
    expect(distanceTo(points, 206, 50)).toBe(0);
    expect(distanceTo(points, 210, 50)).toBe(4);
  });

  it("index where each activity is, once per arrangement", () => {
    const lines = Int32Array.from([-1, 0, 2, 1]);
    const index = lineIndex([10, 11, 12], lines);
    expect([...index]).toEqual([
      [10, 1],
      [12, 2],
      [11, 3],
    ]);
    expect(lineIndex([10, 11, 12], lines)).toBe(index);
  });

  it("index each activity's relationships", () => {
    const links = { count: 2, id: [1, 2], predecessor: [10, 11], successor: [11, 12], type: ["FS", "FS"], lag_hours: [0, 0], driving: [true, false] } as Links;
    const index = linksByActivity(links);
    expect(index.get(11)).toEqual([0, 1]);
    expect(index.get(10)).toEqual([0]);
    expect(linksByActivity(links)).toBe(index);
  });
});
