import { afterEach, describe, expect, it } from "vitest";

import { sample } from "../edit/fixture";
import { COLUMN_BY_ID, comparedKey, showCompared, showUnits, spanText, spanWords } from "./columns";

afterEach(() => {
  showCompared(null);
  showUnits("days");
});

describe("durations and float", () => {
  it("show in days of the activity's calendar", () => {
    expect(spanText(504, 8)).toBe("63");
    expect(spanText(40, 10)).toBe("4");
    expect(spanText(36, 8)).toBe("4.5");
    expect(spanText(10, 24)).toBe("0.42");
  });

  it("show in hours, marked, where the calendar's day isn't known", () => {
    expect(spanText(40, null)).toBe("40h");
    expect(spanText(null, 8)).toBe("");
  });

  it("show in hours when asked, with the titles to match", () => {
    const rows = sample({ hours_per_day: [8, 8, 10], total_float_hours: [16, 0, 40] });
    const float = COLUMN_BY_ID.get("float");
    expect(float?.title).toBe("Total float (d)");
    expect(float?.text(rows, 0)).toBe("2");
    expect(float?.text(rows, 2)).toBe("4");
    showUnits("hours");
    expect(float?.title).toBe("Total float (h)");
    expect(float?.text(rows, 0)).toBe("16");
  });
});

describe("spans in words, for tooltips", () => {
  it("say days where they're known, and hours otherwise or when chosen", () => {
    expect(spanWords(16, 8)).toBe("2 days");
    expect(spanWords(8, 8)).toBe("1 day");
    expect(spanWords(16, null)).toBe("16 h");
    showUnits("hours");
    expect(spanWords(16, 8)).toBe("16 h");
  });
});

const text = (id: "before" | "after", i: number, rows = sample({ hours_per_day: [8, 8, 8] })) =>
  COLUMN_BY_ID.get(id)?.text(rows, i);

describe("the Before and After columns", () => {
  it("show what a comparison found on each activity, spans in the units shown", () => {
    showCompared({
      projects: new Map([[7, "P"]]),
      activities: new Map([
        [comparedKey("P", "A2"), { added: false, fields: [{ field: "original_duration", from: "8h", to: "32h" }, { field: "calendar", from: "Five day", to: "" }] }],
        [comparedKey("P", "A3"), { added: true, fields: [] }],
        // The same code in another project isn't this one.
        [comparedKey("Q", "A1"), { added: true, fields: [] }],
      ]),
    });
    expect(text("before", 1)).toBe("Original duration 1 day; Calendar Five day");
    expect(text("after", 1)).toBe("Original duration 4 days; Calendar none");
    expect([text("before", 2), text("after", 2)]).toEqual(["", "Added"]);
    expect([text("before", 0), text("after", 0)]).toEqual(["", ""]);
    showUnits("hours");
    expect(text("after", 1)).toBe("Original duration 32 h; Calendar none");
  });

  it("are empty with no comparison", () => {
    expect([text("before", 1), text("after", 1)]).toEqual(["", ""]);
  });
});
