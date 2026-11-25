import { afterEach, describe, expect, it } from "vitest";

import { DATE_STYLES, DEFAULT_DATES, dateText, moveText, showDates } from "./dates";

afterEach(() => showDates(DEFAULT_DATES));

describe("dates as shown", () => {
  it("open as 05 May 26, with no time", () => {
    expect(DEFAULT_DATES).toEqual({ style: "dd mmm yy", time: false });
    expect(dateText("2026-05-05 10:40")).toBe("05 May 26");
  });

  it("write the day in each style, with the time when it's shown", () => {
    const written = DATE_STYLES.map(({ style }) => {
      showDates({ style, time: false });
      const day = dateText("2026-05-31 08:00");
      showDates({ style, time: true });
      return [day, dateText("2026-05-31 08:00")];
    });
    expect(written).toEqual([
      ["31 May 26", "31 May 26 08:00"],
      ["31-May-26", "31-May-26 08:00"],
      ["2026-05-31", "2026-05-31 08:00"],
      ["31/05/2026", "31/05/2026 08:00"],
      ["05/31/2026", "05/31/2026 08:00"],
    ]);
    // Each example in the menu is what it shows.
    for (const { style, example } of DATE_STYLES) {
      showDates({ style, time: false });
      expect(dateText("2026-05-31 08:00")).toBe(example);
    }
  });

  it("leave out what isn't there", () => {
    showDates({ style: "dd mmm yy", time: true });
    expect(dateText("2026-05-31")).toBe("31 May 26");
    expect(dateText(null)).toBe("");
    expect(dateText("")).toBe("");
    expect(dateText("none")).toBe("none");
    // Asked for the day alone.
    expect(dateText("2026-05-31 08:00", false)).toBe("31 May 26");
  });

  it("keep the times of a move within a day, so it reads as a move", () => {
    expect(moveText("2026-01-15 17:00", "2026-01-15 09:00")).toEqual(["15 Jan 26 17:00", "15 Jan 26 09:00"]);
    expect(moveText("2026-01-15 17:00", "2026-01-16 17:00")).toEqual(["15 Jan 26", "16 Jan 26"]);
    expect(moveText(null, "2026-01-16 17:00")).toEqual(["", "16 Jan 26"]);
  });
});
