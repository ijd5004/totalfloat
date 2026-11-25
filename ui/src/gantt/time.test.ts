import { describe, expect, it } from "vitest";

import { DEFAULT_ZOOM, ZOOMS, fitZoom, stepZoom, labelOf, minutesAt, minutesOf, nextUnit, scaleOf, ticks, unitStart, xOf } from "./time";

const at = (text: string) => minutesOf(text);

describe("dates on the axis", () => {
  it("reads the server's dates as civil minutes, with no time zone", () => {
    expect(at("1970-01-01 00:00")).toBe(0);
    expect(at("1970-01-02 08:30")).toBe(24 * 60 + 8 * 60 + 30);
    expect(at(null as unknown as string)).toBeNaN();
    expect(at("")).toBeNaN();
  });

  it("maps dates to pixels linearly at each zoom, and back", () => {
    const origin = at("2026-01-01 00:00");
    for (const zoom of ZOOMS) {
      const scale = scaleOf(origin, zoom);
      expect(xOf(scale, origin)).toBe(0);
      // A day is the zoom's width, wherever it falls; a working day is no different.
      expect(xOf(scale, at("2026-01-02 00:00"))).toBeCloseTo(zoom.pxPerDay, 9);
      expect(xOf(scale, at("2026-03-01 12:00")) - xOf(scale, at("2026-02-28 12:00"))).toBeCloseTo(zoom.pxPerDay, 9);
      expect(minutesAt(scale, xOf(scale, at("2026-07-04 13:45")))).toBeCloseTo(at("2026-07-04 13:45"), 6);
    }
  });
});

describe("timescale units", () => {
  it("start where each unit starts, with weeks on Monday", () => {
    const thursday = at("2026-01-08 15:00");
    expect(unitStart(thursday, "day")).toBe(at("2026-01-08 00:00"));
    expect(unitStart(thursday, "week")).toBe(at("2026-01-05 00:00"));
    expect(unitStart(at("2026-01-04 10:00"), "week")).toBe(at("2025-12-29 00:00"));
    expect(unitStart(thursday, "month")).toBe(at("2026-01-01 00:00"));
    expect(unitStart(at("2026-05-20 00:00"), "quarter")).toBe(at("2026-04-01 00:00"));
    expect(unitStart(thursday, "year")).toBe(at("2026-01-01 00:00"));
  });

  it("step to the next unit across month and year ends", () => {
    expect(nextUnit(at("2026-01-31 00:00"), "day")).toBe(at("2026-02-01 00:00"));
    expect(nextUnit(at("2026-12-28 00:00"), "week")).toBe(at("2027-01-04 00:00"));
    expect(nextUnit(at("2026-12-01 00:00"), "month")).toBe(at("2027-01-01 00:00"));
    expect(nextUnit(at("2026-10-01 00:00"), "quarter")).toBe(at("2027-01-01 00:00"));
  });

  it("label each tier", () => {
    expect(labelOf(at("2026-04-01 00:00"), "quarter", "top")).toBe("Q2 2026");
    expect(labelOf(at("2026-04-01 00:00"), "quarter", "bottom")).toBe("Q2");
    expect(labelOf(at("2026-02-01 00:00"), "month", "top")).toBe("Feb 2026");
    expect(labelOf(at("2026-02-01 00:00"), "month", "bottom")).toBe("Feb");
    expect(labelOf(at("2026-01-05 00:00"), "week", "top")).toBe("5 Jan 2026");
    expect(labelOf(at("2026-01-05 00:00"), "week", "bottom")).toBe("5");
    expect(labelOf(at("2026-01-05 00:00"), "day", "bottom")).toBe("M 5");
    expect(labelOf(at("2026-01-05 00:00"), "day", "top")).toBe("Mon 5 Jan");
  });
});

describe("tier labels at each zoom level", () => {
  const origin = at("2026-01-01 00:00");
  // What a 1,200-pixel-wide Gantt shows from the start of 2026, top tier then bottom.
  const expected: [string[], string[]][] = [
    [["2026", "2027", "2028"], ["Q1", "Q2", "Q3", "Q4", "Q1"]],
    [["2026", "2027", "2028", "2029"], ["Jan", "Feb", "Mar", "Apr", "May"]],
    [["Q1 2026", "Q2 2026", "Q3 2026", "Q4 2026", "Q1 2027"], ["Jan", "Feb", "Mar", "Apr", "May"]],
    [["Jan 2026", "Feb 2026", "Mar 2026", "Apr 2026", "May 2026"], ["29", "5", "12", "19", "26"]],
    [["Jan 2026", "Feb 2026"], ["T 1", "F 2", "S 3", "S 4", "M 5"]],
    [["29 Dec 2025", "5 Jan 2026", "12 Jan 2026"], ["T 1", "F 2", "S 3", "S 4", "M 5"]],
  ];

  ZOOMS.forEach((zoom, n) => {
    it(`shows ${zoom.top} over ${zoom.bottom} at ${zoom.pxPerDay} px a day`, () => {
      const scale = scaleOf(origin, zoom);
      const top = ticks(scale, zoom.top, "top", 0, 1200).map((t) => t.label);
      const bottom = ticks(scale, zoom.bottom, "bottom", 0, 1200).map((t) => t.label);
      const [wantTop, wantBottom] = expected[n] ?? [[], []];
      expect(top.slice(0, wantTop.length)).toEqual(wantTop);
      expect(bottom.slice(0, wantBottom.length)).toEqual(wantBottom);
      // Cells sit edge to edge, each as wide as its unit at this zoom.
      const cells = ticks(scale, zoom.bottom, "bottom", 0, 1200);
      for (let k = 1; k < cells.length; k++) {
        const [before, after] = [cells[k - 1], cells[k]];
        expect(before && after && before.x + before.width).toBeCloseTo(after?.x ?? 0, 9);
      }
    });
  });

  it("opens fitted to the project", () => {
    expect(DEFAULT_ZOOM).toBe("fit");
  });
});

describe("the fit zoom", () => {
  const at = minutesOf;
  it("puts the first start and last finish inside the width, a margin from each edge", () => {
    const first = at("2026-01-05 08:00");
    const last = at("2028-06-30 17:00");
    const fitted = fitZoom(first, last, 900);
    expect(fitted).not.toBeNull();
    if (!fitted) return;
    const scale = scaleOf(fitted.left, fitted.zoom);
    expect(xOf(scale, first)).toBeGreaterThan(0);
    expect(xOf(scale, last)).toBeLessThan(900);
    // The margins are small: the span takes over nine tenths of the width.
    expect(xOf(scale, last) - xOf(scale, first)).toBeGreaterThan(900 * 0.9);
    // Over two and a half years at 900 px, the tiers are the nearest fixed level's: year over month.
    expect(fitted.zoom).toMatchObject({ top: "year", bottom: "month" });
  });

  it("keeps a day's margin around a short span", () => {
    const fitted = fitZoom(at("2026-03-02 08:00"), at("2026-03-02 17:00"), 600);
    expect(fitted && fitted.left).toBe(at("2026-03-02 08:00") - 24 * 60);
  });

  it("has nothing to fit without dates or width", () => {
    expect(fitZoom(Number.NaN, at("2026-03-02 17:00"), 600)).toBeNull();
    expect(fitZoom(at("2026-03-02 08:00"), at("2026-03-02 17:00"), 0)).toBeNull();
  });

  it("steps from fit to the nearest fixed level past it, either way", () => {
    // ZOOMS: 0.25, 1, 3, 8, 24, 60 px a day.
    expect(stepZoom("fit", 2, 1)).toBe(2);
    expect(stepZoom("fit", 2, -1)).toBe(1);
    expect(stepZoom("fit", 3, 1)).toBe(3);
    expect(stepZoom("fit", 3, -1)).toBe(1);
    // Past the last level that way, it stays fitted rather than step back.
    expect(stepZoom("fit", 100, 1)).toBe("fit");
    expect(stepZoom("fit", 100, -1)).toBe(5);
    expect(stepZoom("fit", 0.1, -1)).toBe("fit");
    expect(stepZoom("fit", 0.1, 1)).toBe(0);
    expect(stepZoom(3, 2, 1)).toBe(4);
    expect(stepZoom(0, 2, -1)).toBe(0);
  });
});
