import { describe, expect, it } from "vitest";

import { dropAt, edgeAt, typeOf } from "./linking";

// A1's bar runs 100 to 200 on the first row; A2's 300 to 400 on the second;
// M3 is a milestone at 500 on the third.
const BARS = [
  { id: 1, left: 100, right: 200, top: 0, bottom: 22 },
  { id: 2, left: 300, right: 400, top: 22, bottom: 44 },
  { id: 3, left: 495, right: 505, top: 44, bottom: 66 },
];

describe("drawing a link on the Gantt", () => {
  it("takes the type from the two ends", () => {
    expect(typeOf("finish", "start")).toBe("FS");
    expect(typeOf("start", "start")).toBe("SS");
    expect(typeOf("finish", "finish")).toBe("FF");
    expect(typeOf("start", "finish")).toBe("SF");
  });

  it("finds a bar's start or finish near its edge, and nothing in its middle", () => {
    expect(edgeAt(BARS, 103, 10)).toEqual({ id: 1, end: "start", x: 100, y: 11 });
    expect(edgeAt(BARS, 205, 10)).toEqual({ id: 1, end: "finish", x: 200, y: 11 });
    expect(edgeAt(BARS, 150, 10)).toBeNull();
    // Another row's bar isn't this row's.
    expect(edgeAt(BARS, 300, 10)).toBeNull();
  });

  it("splits a milestone down its middle", () => {
    expect(edgeAt(BARS, 497, 50)?.end).toBe("start");
    expect(edgeAt(BARS, 503, 50)?.end).toBe("finish");
  });

  it("drops anywhere on another bar, on its nearer end, but never on the bar dragged from", () => {
    expect(dropAt(BARS, 320, 30, 1)).toEqual({ id: 2, end: "start", x: 300, y: 33 });
    expect(dropAt(BARS, 390, 30, 1)).toEqual({ id: 2, end: "finish", x: 400, y: 33 });
    expect(dropAt(BARS, 150, 10, 1)).toBeNull();
    expect(dropAt(BARS, 250, 30, 1)).toBeNull();
  });
});
