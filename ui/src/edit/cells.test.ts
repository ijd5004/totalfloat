import { afterEach, describe, expect, it } from "vitest";

import { showUnits } from "../grid/columns";

import { type Built, buildBatch, cellKey, constraintKind, dateTime, inputText, span, withUnits } from "./cells";
import { sample } from "./fixture";

function batchOf(built: Built) {
  if (!("batch" in built)) throw new Error(`refused: ${JSON.stringify([...built.refused])}`);
  return built.batch;
}

function refusedOf(built: Built) {
  if (!("refused" in built)) throw new Error(`not refused: ${JSON.stringify(built.batch)}`);
  return built.refused;
}

describe("typed values", () => {
  it("reads durations as the API takes them, with bare numbers in hours", () => {
    expect(span("5d")).toBe("5d");
    expect(span(" 40 ")).toBe("40h");
    expect(span("7.5H")).toBe("7.5h");
    expect(span("0.25d")).toBe("0.25d");
    // Weeks, months and years, by the calendar's hours in each.
    expect(span("2W")).toBe("2w");
    expect(span("1m")).toBe("1m");
    expect(span("1y")).toBe("1y");
    for (const bad of ["5x", "d", "", "5 days", "1.2345h"]) {
      expect(span(bad)).toMatchObject({ hint: expect.stringContaining("5d") });
    }
  });

  it("reads dates and times, and refuses ones that don't exist", () => {
    expect(dateTime("2026-03-02 08:00")).toBe("2026-03-02 08:00");
    expect(dateTime("2026-03-02T08:00")).toBe("2026-03-02 08:00");
    for (const bad of ["2026-02-30 08:00", "2026-03-02", "2026-03-02 24:00", "tomorrow"]) {
      expect(dateTime(bad)).toMatchObject({ hint: "type it as 2026-03-02 08:00" });
    }
  });

  it("reads constraint kinds in the server's words or a person's", () => {
    expect(constraintKind("Start on or after")).toBe("start-on-or-after");
    expect(constraintKind("finish_on")).toBe("finish-on");
    expect(constraintKind("")).toBeNull();
    expect(constraintKind("none")).toBeNull();
    expect(constraintKind("sometime")).toMatchObject({ message: expect.stringContaining("sometime") });
  });

  it("opens duration inputs with their unit", () => {
    expect(inputText(sample(), 0, "original")).toBe("40h");
    expect(inputText(sample(), 0, "name")).toBe("Name A1");
  });
});

describe("spans in days", () => {
  const days = sample({ hours_per_day: [8, 8, 10] });

  it("open in days where they're shown and exact, else hours", () => {
    expect(inputText(days, 0, "original")).toBe("5d");
    expect(inputText(sample({ hours_per_day: [8, 8, 8], original_duration_hours: [36, 40, 40] }), 0, "original")).toBe("4.5d");
    // 36.5 hours is 4.5625 days, more than a span's three decimals hold: hours, then.
    expect(inputText(sample({ hours_per_day: [8, 8, 8], original_duration_hours: [36.5, 40, 40] }), 0, "original")).toBe("36.5h");
    expect(inputText(sample({ hours_per_day: [7, 8, 8], original_duration_hours: [10, 40, 40] }), 0, "original")).toBe("10h");
  });

  it("send weeks, months and years to the server, which knows the calendar's hours in each", () => {
    // A one-hour activity: `1w`, `1m` or `1y` isn't one hour, whatever the numbers say.
    const short = sample({ hours_per_day: [8, 8, 8], original_duration_hours: [1, 40, 40], remaining_duration_hours: [1, 40, 40] });
    for (const text of ["1w", "1m", "1y"]) {
      expect(batchOf(buildBatch(short, [{ id: 1, column: "original", text }])).edits).toEqual([
        { op: "set_duration", activity: "A1", original: text },
      ]);
      expect(batchOf(buildBatch(short, [{ id: 1, column: "remaining", text }])).edits).toEqual([
        { op: "set_duration", activity: "A1", remaining: text },
      ]);
    }
  });

  it("read a bare number in days where days are shown", () => {
    const batch = batchOf(buildBatch(days, [{ id: 1, column: "original", text: "6" }]));
    expect(batch.edits).toEqual([{ op: "set_duration", activity: "A1", original: "6d" }]);
  });

  it("count 5, 5d and 40h as the 40 hours they are, so nothing changes", () => {
    for (const text of ["5", "5d", "40h"]) {
      expect(batchOf(buildBatch(days, [{ id: 1, column: "original", text }])).edits).toEqual([]);
    }
  });

  afterEach(() => showUnits("days"));

  it("change nothing when a rounded cell is copied and pasted back", () => {
    // 36.5 hours on an 8-hour day shows, and copies, as 4.56 days; 4.56d would be 36.48 hours.
    const rounded = sample({ hours_per_day: [8, 8, 8], original_duration_hours: [36.5, 40, 40] });
    expect(batchOf(buildBatch(rounded, withUnits(rounded, [{ id: 1, column: "original", text: "4.56" }]))).edits).toEqual([]);
    // Typed with its unit, it's what was asked for.
    expect(batchOf(buildBatch(rounded, [{ id: 1, column: "original", text: "4.56d" }])).edits).toEqual([
      { op: "set_duration", activity: "A1", original: "4.56d" },
    ]);
  });

  it("compare to the minute the server keeps", () => {
    const close = sample({ hours_per_day: [8, 8, 8], original_duration_hours: [40.004, 40, 40] });
    expect(batchOf(buildBatch(close, [{ id: 1, column: "original", text: "40h" }])).edits).toEqual([]);
    const minute = sample({ hours_per_day: [8, 8, 8], original_duration_hours: [40 + 1 / 60, 40, 40] });
    expect(batchOf(buildBatch(minute, [{ id: 1, column: "original", text: "40h" }])).edits).toHaveLength(1);
  });

  it("send a remaining kept with a new original as stored, not as the grid rounds it", () => {
    // Not started: 36.5 hours remaining shows as 4.56 days; the row is pasted back with a new original.
    const rounded = sample({ hours_per_day: [8, 8, 8], original_duration_hours: [36.5, 40, 40], remaining_duration_hours: [36.5, 40, 40] });
    const pasted = withUnits(rounded, [
      { id: 1, column: "original", text: "6" },
      { id: 1, column: "remaining", text: "4.56" },
    ]);
    expect(batchOf(buildBatch(rounded, pasted)).edits).toEqual([
      { op: "set_duration", activity: "A1", original: "6d", remaining: "36.5h" },
    ]);
  });

  it("keep the unit a bare number was typed in when the units shown change", () => {
    const typed = withUnits(days, [{ id: 1, column: "original", text: "6" }]);
    showUnits("hours");
    expect(batchOf(buildBatch(days, typed)).edits).toEqual([{ op: "set_duration", activity: "A1", original: "6d" }]);
    // Typed now, it's hours.
    expect(batchOf(buildBatch(days, withUnits(days, [{ id: 1, column: "original", text: "6" }]))).edits).toEqual([
      { op: "set_duration", activity: "A1", original: "6h" },
    ]);
  });

  it("send days typed with a new calendar, even if they match the old one's", () => {
    // 40 hours is 5 days of 8 hours; on a 10-hour calendar, 5 days is 50 hours.
    const batch = batchOf(
      buildBatch(days, [
        { id: 1, column: "calendar", text: "Ten hour" },
        { id: 1, column: "original", text: "5d" },
      ]),
    );
    expect(batch.edits).toEqual([
      { op: "set_calendar", activity: "A1", calendar: "Ten hour" },
      { op: "set_duration", activity: "A1", original: "5d" },
    ]);
    // Hours are hours on any calendar.
    expect(
      batchOf(
        buildBatch(days, [
          { id: 1, column: "calendar", text: "Ten hour" },
          { id: 1, column: "original", text: "40h" },
        ]),
      ).edits,
    ).toEqual([{ op: "set_calendar", activity: "A1", calendar: "Ten hour" }]);
  });
});

describe("a batch from typed cells", () => {
  it("makes one edit per kind of change, with the cells each came from", () => {
    const batch = batchOf(
      buildBatch(sample(), [
        { id: 1, column: "original", text: "5d" },
        { id: 1, column: "remaining", text: "24" },
        { id: 2, column: "name", text: " Pour slab " },
        { id: 3, column: "calendar", text: "Five day" },
      ]),
    );
    expect(batch.edits).toEqual([
      { op: "set_duration", activity: "A1", original: "5d", remaining: "24h" },
      { op: "set_name", activity: "A2", name: "Pour slab" },
      { op: "set_calendar", activity: "A3", calendar: "Five day" },
    ]);
    expect(batch.cells).toEqual([[cellKey(1, "original"), cellKey(1, "remaining")], [cellKey(2, "name")], [cellKey(3, "calendar")]]);
    expect(batch.project).toBe(7);
  });

  it("drops cells left as they were, so a row pasted back changes only what differs", () => {
    const rows = sample();
    const batch = batchOf(
      buildBatch(rows, [
        { id: 1, column: "code", text: "A1" },
        { id: 1, column: "name", text: "Name A1" },
        { id: 1, column: "original", text: "40" },
        { id: 1, column: "start", text: "2026-01-05 08:00" },
        { id: 1, column: "remaining", text: "16h" },
      ]),
    );
    expect(batch.edits).toEqual([{ op: "set_duration", activity: "A1", remaining: "16h" }]);
  });

  it("refuses the whole batch when any cell can't be sent, marking each such cell", () => {
    const refused = refusedOf(
      buildBatch(sample(), [
        { id: 1, column: "original", text: "5x" },
        { id: 2, column: "name", text: "Fine" },
        { id: 3, column: "finish", text: "2026-02-01 17:00" },
        { id: 99, column: "name", text: "Gone" },
      ]),
    );
    expect([...refused.keys()].sort()).toEqual([cellKey(1, "original"), cellKey(3, "finish"), cellKey(99, "name")].sort());
    expect(refused.get(cellKey(3, "finish"))?.message).toBe("Finish can't be changed here");
  });

  it("puts actuals and a remaining typed with them in one progress edit, before durations", () => {
    const batch = batchOf(
      buildBatch(sample(), [
        { id: 1, column: "original", text: "48h" },
        { id: 1, column: "actual_start", text: "2026-01-05 08:00" },
        { id: 2, column: "actual_start", text: "2026-01-05 08:00" },
        { id: 2, column: "actual_finish", text: "2026-01-09 17:00" },
        { id: 2, column: "remaining", text: "0" },
      ]),
    );
    // A remaining on its own after an actual finish would land on finished work.
    expect(batch.edits).toEqual([
      { op: "set_progress", activity: "A1", actual_start: "2026-01-05 08:00" },
      { op: "set_duration", activity: "A1", original: "48h" },
      { op: "set_progress", activity: "A2", actual_start: "2026-01-05 08:00", actual_finish: "2026-01-09 17:00", remaining: "0h" },
    ]);
    expect(batch.cells[2]).toEqual([cellKey(2, "actual_start"), cellKey(2, "actual_finish"), cellKey(2, "remaining")]);
    expect(refusedOf(buildBatch(sample({ actual_start: ["2026-01-05 08:00", null, null] }), [{ id: 1, column: "actual_start", text: "" }])).size).toBe(1);
  });

  it("joins a constraint's kind and date, filling either from what's set", () => {
    const rows = sample({
      constraint: ["start-on-or-after", null, "as-late-as-possible"],
      constraint_date: ["2026-01-05 08:00", null, null],
    });
    expect(batchOf(buildBatch(rows, [{ id: 1, column: "constraint_date", text: "2026-02-02 08:00" }])).edits).toEqual([
      { op: "set_constraint", activity: "A1", constraint: "start-on-or-after", date: "2026-02-02 08:00" },
    ]);
    expect(batchOf(buildBatch(rows, [{ id: 1, column: "constraint", text: "finish on" }])).edits).toEqual([
      { op: "set_constraint", activity: "A1", constraint: "finish-on", date: "2026-01-05 08:00" },
    ]);
    expect(batchOf(buildBatch(rows, [{ id: 1, column: "constraint", text: "" }])).edits).toEqual([
      { op: "clear_constraint", activity: "A1" },
    ]);
    expect(
      batchOf(
        buildBatch(rows, [
          { id: 2, column: "constraint", text: "mandatory-start" },
          { id: 2, column: "constraint_date", text: "2026-03-02 08:00" },
        ]),
      ),
    ).toMatchObject({ edits: [{ op: "set_constraint", constraint: "mandatory-start", date: "2026-03-02 08:00" }], cells: [[cellKey(2, "constraint"), cellKey(2, "constraint_date")]] });
    // A kind that needs a date, without one; a date with no constraint; a date on as late as possible.
    expect(refusedOf(buildBatch(rows, [{ id: 2, column: "constraint", text: "start-on" }])).get(cellKey(2, "constraint"))?.message).toBe(
      "start-on needs a date",
    );
    expect(refusedOf(buildBatch(rows, [{ id: 2, column: "constraint_date", text: "2026-03-02 08:00" }])).has(cellKey(2, "constraint_date"))).toBe(true);
    expect(refusedOf(buildBatch(rows, [{ id: 3, column: "constraint_date", text: "2026-03-02 08:00" }])).has(cellKey(3, "constraint_date"))).toBe(true);
    // No constraint, with a date: the date isn't dropped without a word.
    expect(
      refusedOf(
        buildBatch(rows, [
          { id: 1, column: "constraint", text: "none" },
          { id: 1, column: "constraint_date", text: "2026-03-02 08:00" },
        ]),
      ).get(cellKey(1, "constraint_date"))?.message,
    ).toBe("with no constraint, there's no date to set");
  });

  it("leaves the project out when edits span several", () => {
    const batch = batchOf(
      buildBatch(sample({ project: [7, 8, 8] }), [
        { id: 1, column: "name", text: "X" },
        { id: 2, column: "name", text: "Y" },
      ]),
    );
    expect(batch.project).toBeUndefined();
  });

  it("refuses edits across projects to an ID that's in more than one", () => {
    // A1 in project 7 and A1 in project 8 can't be told apart without a project.
    const rows = sample({ project: [7, 8, 8], code: ["A1", "A1", "A3"] });
    const refused = refusedOf(
      buildBatch(rows, [
        { id: 1, column: "name", text: "X" },
        { id: 2, column: "name", text: "Y" },
      ]),
    );
    expect(refused.get(cellKey(1, "name"))?.message).toBe("A1 is an activity ID in more than one project");
    expect(refused.has(cellKey(2, "name"))).toBe(true);
    // Within one project, the request names it.
    expect(batchOf(buildBatch(rows, [{ id: 2, column: "name", text: "Y" }])).project).toBe(8);
  });

  it("moves an activity's calendar before reading its durations in it", () => {
    const batch = batchOf(
      buildBatch(sample(), [
        { id: 1, column: "original", text: "1d" },
        { id: 1, column: "remaining", text: "1d" },
        { id: 1, column: "actual_start", text: "2026-01-05 08:00" },
        { id: 1, column: "calendar", text: "Seven day" },
      ]),
    );
    expect(batch.edits.map((edit) => edit.op)).toEqual(["set_calendar", "set_progress", "set_duration"]);
  });

  it("sends a remaining as it was with a new original only on work not started", () => {
    const done = sample({ status: ["completed", "completed", "completed"], remaining_duration_hours: [0, 0, 0] });
    expect(
      batchOf(
        buildBatch(done, [
          { id: 1, column: "original", text: "16" },
          { id: 1, column: "remaining", text: "0" },
        ]),
      ).edits,
    ).toEqual([{ op: "set_duration", activity: "A1", original: "16h" }]);
  });

  it("sends a remaining typed with a new original, though it's as it was", () => {
    // Not started, 40h each: remaining would otherwise follow original to 80h.
    const batch = batchOf(
      buildBatch(sample(), [
        { id: 1, column: "original", text: "80" },
        { id: 1, column: "remaining", text: "40" },
      ]),
    );
    expect(batch.edits).toEqual([{ op: "set_duration", activity: "A1", original: "80h", remaining: "40h" }]);
  });

  it("keeps only the cells that change something, or were refused", () => {
    const built = buildBatch(sample(), [
      { id: 1, column: "name", text: "Name A1" },
      { id: 1, column: "original", text: "5d" },
      { id: 2, column: "finish", text: "2026-01-09 17:00" },
      { id: 2, column: "original", text: "5x" },
    ]);
    expect(built.kept).toEqual([
      { id: 1, column: "original", text: "5d" },
      { id: 2, column: "original", text: "5x" },
    ]);
  });
});
