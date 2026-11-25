import { describe, expect, it } from "vitest";

import { FOOTER_HEIGHT, HEADER_HEIGHT, MARGIN, MIN_SCALE, ROW_HEIGHT, TITLE_HEIGHT, columnWidths, cssPageSize, dayOf, fitText, pageSize, paginate, rowsPerPage, tiersFor } from "./pages";

describe("printed pages", () => {
  it("are the paper's size, turned for landscape", () => {
    expect(pageSize("a4", "portrait")).toEqual({ width: 595.28, height: 841.89 });
    expect(pageSize("letter", "landscape")).toEqual({ width: 792, height: 612 });
    expect(cssPageSize("tabloid", "landscape")).toBe("17in 11in");
  });

  it("hold whole rows between the header and footer, and split into pages of them", () => {
    const per = rowsPerPage(612);
    const body = 612 - 2 * MARGIN - TITLE_HEIGHT - HEADER_HEIGHT - FOOTER_HEIGHT;
    expect(per * ROW_HEIGHT).toBeLessThanOrEqual(body);
    expect((per + 1) * ROW_HEIGHT).toBeGreaterThan(body);
    const pages = paginate(Array.from({ length: 500 }, (_, k) => k), per);
    expect(pages.flat()).toHaveLength(500);
    expect(pages.every((page) => page.length <= per)).toBe(true);
    expect(pages.length).toBe(Math.ceil(500 / per));
    expect(paginate([], per)).toEqual([[]]);
  });

  it("fit the grid to its share of the width, or keep its screen widths", () => {
    const fitted = columnWidths([100, 300], 700, true, 0.5);
    expect(fitted.kept).toBe(2);
    expect(fitted.widths.reduce((a, b) => a + b, 0)).toBeCloseTo(350);
    expect((fitted.widths[1] ?? 0) / (fitted.widths[0] ?? 1)).toBeCloseTo(3);
    expect(columnWidths([100, 300], 700, false, 0.5)).toEqual({ widths: [75, 225], kept: 2 });
  });

  it("leave columns off the right rather than shrink them past reading", () => {
    // Thirteen columns of 120 into 300 points would be a fifth of their size.
    const many = columnWidths(Array.from({ length: 13 }, () => 120), 600, true, 0.5);
    expect(many.kept).toBe(4);
    expect(many.widths.reduce((a, b) => a + b, 0)).toBeCloseTo(300);
    expect(Math.min(...many.widths)).toBeGreaterThanOrEqual(120 * MIN_SCALE);
    // One very wide column is kept, whatever.
    const wide = columnWidths([2000], 300, false, 0.5);
    expect(wide.kept).toBe(1);
    expect(wide.widths[0]).toBeCloseTo(225);
  });

  it("choose the timescale's tiers by the range", () => {
    expect(tiersFor(30)).toEqual({ top: "month", bottom: "day" });
    expect(tiersFor(120)).toEqual({ top: "month", bottom: "week" });
    expect(tiersFor(700)).toEqual({ top: "year", bottom: "month" });
    expect(tiersFor(3000)).toEqual({ top: "year", bottom: "quarter" });
  });

  it("cut text to fit, and read a date input as the server writes dates", () => {
    expect(fitText("Excavate", 100, 7)).toBe("Excavate");
    expect(fitText("A very long activity name indeed", 40, 7)).toMatch(/…$/);
    expect(fitText("A very long activity name indeed", 40, 7).length).toBeLessThan(15);
    // Measured, a wide face fits fewer characters than the estimate allows.
    const wide = (t: string) => t.length * 6.6;
    expect(fitText("WWWWWWWWWWWWWWW", 56.5, 7)).toBe("WWWWWWWWWWWWWWW");
    expect(wide(fitText("WWWWWWWWWWWWWWW", 56.5, 7, wide))).toBeLessThanOrEqual(56.5);
    expect(fitText("WWWWWWWWWWWWWWW", 56.5, 7, wide)).toMatch(/…$/);
    expect(dayOf("2026-03-02")).toBe("2026-03-02 00:00");
    expect(dayOf("")).toBeNull();
  });
});
