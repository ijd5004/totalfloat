import { describe, expect, it } from "vitest";

import type { EditReport, Summary } from "../api";
import editReport from "../api/golden/edit-1.json";
import { golden } from "../api/schemas/golden";
import { inspectOf } from "../api/testing";
import { sample } from "../edit/fixture";
import { movedText, outlookOf } from "./outlook";

const SUMMARY = {
  ...golden["summary-1"],
  inspect: inspectOf("plant.tfdb", [
    { id: 7, short_name: "P", name: "Plant", data_date: "2026-01-05 08:00", finish: "2026-01-09 17:00", activities: 3 },
  ]),
} as Summary;

describe("the title block's finish and float", () => {
  it("shows the lowest total float of unfinished work, in days of its calendar", () => {
    const rows = sample({ total_float_hours: [16, -8, 0], hours_per_day: [8, 8, 8], status: ["not-started", "completed", "in-progress"] });
    // A2 is finished, so its float doesn't count.
    expect(outlookOf(rows, SUMMARY, null)).toEqual({ float: "0d" });
    expect(outlookOf(sample({ total_float_hours: [16, -8, 0], hours_per_day: [8, 8, 8] }), SUMMARY, null).float).toBe("−1d");
    expect(outlookOf(null, SUMMARY, null)).toEqual({ float: null });
  });

  it("with edits waiting, gives the finish they give, how far it moves, and the float they leave", () => {
    const rows = sample({ total_float_hours: [16, 0, 0], hours_per_day: [8, 8, 8] });
    const template = (editReport as unknown as EditReport).projects[0];
    if (!template) throw new Error("the golden edit report has no project");
    const preview = {
      ...(editReport as unknown as EditReport),
      projects: [
        {
          ...template,
          id: 7,
          previous_finish: "2026-01-09 17:00",
          scheduled_finish: "2026-01-13 17:00",
          changed_dates_or_float: 2,
          changes: {
            ...template.changes,
            activities: [
              { code: "A2", name: "Name A2", fields: [{ field: "total_float", from: "0h", to: "-32h" }] },
              { code: "A3", name: "Name A3", fields: [{ field: "early_finish", from: "2026-01-09 17:00", to: "2026-01-13 17:00" }] },
            ],
          },
        },
      ],
    } as EditReport;
    expect(outlookOf(rows, SUMMARY, preview)).toEqual({
      float: "0d",
      pending: { finish: "2026-01-13 17:00", moved: "+4d", float: "−4d", lower: true },
    });
  });

  it("shows no float while edits wait when the preview's list of changes was cut short", () => {
    const rows = sample({ total_float_hours: [16, 0, 0], hours_per_day: [8, 8, 8] });
    const template = (editReport as unknown as EditReport).projects[0];
    if (!template) throw new Error("the golden edit report has no project");
    const preview = {
      ...(editReport as unknown as EditReport),
      projects: [
        {
          ...template,
          id: 7,
          previous_finish: "2026-01-09 17:00",
          scheduled_finish: "2026-01-13 17:00",
          // 30 changed, one listed.
          changed_dates_or_float: 30,
          changes: { ...template.changes, activities: [{ code: "A2", name: "Name A2", fields: [{ field: "total_float", from: "0h", to: "8h" }] }] },
        },
      ],
    } as EditReport;
    expect(outlookOf(rows, SUMMARY, preview).pending).toEqual({ finish: "2026-01-13 17:00", moved: "+4d", float: null, lower: false });
    // Nor when the edits progress, add or delete activities.
    const listed = { ...preview, projects: preview.projects.map((p) => ({ ...p, changed_dates_or_float: 1 })) } as EditReport;
    expect(outlookOf(rows, SUMMARY, listed).pending?.float).toBe("0d");
    expect(outlookOf(rows, SUMMARY, listed, [{ op: "set_progress" }]).pending?.float).toBeNull();
  });

  it("says a move in calendar days, or hours within a day", () => {
    expect(movedText("2026-01-09 17:00", "2026-01-13 17:00")).toBe("+4d");
    expect(movedText("2026-01-09 17:00", "2026-01-09 09:00")).toBe("−8h");
    expect(movedText("2026-01-09 17:00", "2026-01-09 17:00")).toBeNull();
    expect(movedText(null, "2026-01-09 17:00")).toBeNull();
  });
});
