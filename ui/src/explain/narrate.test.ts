import { describe, expect, it } from "vitest";

import type { Explanation, Rows } from "../api";
import report from "./e1040.fixture.json";
import { type Stop, because, days, narrate, readable } from "./narrate";
import { type DateFormat, showDates } from "../dates";

// With times, so the steps say the hour each date falls at.
const WITH_TIMES: DateFormat = { style: "dd mmm yy", time: true };
showDates(WITH_TIMES);

// The demo plant's activities the chain names, by task_id.
const ACTIVITIES: [number, string, string][] = [
  [6, "C1020", "Mass excavation"],
  [8, "C1040", "Pour basin foundations"],
  [9, "C1050", "Cure foundations"],
  [12, "S1010", "Erect clarifier walls"],
  [17, "E1010", "Install pumps"],
  [18, "E1020", "Process piping"],
  [20, "E1040", "Terminate and test"],
];
const rows = { id: ACTIVITIES.map((a) => a[0]), code: ACTIVITIES.map((a) => a[1]), name: ACTIVITIES.map((a) => a[2]) } as unknown as Rows;
const index = new Map(ACTIVITIES.map((a, i) => [a[0], i]));

describe("an explanation in a scheduler's words", () => {
  const told = narrate(report as Explanation, 0, rows, index);

  it("asks the question the panel answers", () => {
    expect(told.question).toBe("Why does E1040 start on 25 Jun 26?");
  });

  it("makes one numbered stop per activity, back to where the chain starts", () => {
    expect(told.stops.map((s: Stop) => `${s.n} ${s.code}`)).toEqual([
      "1 E1040",
      "2 E1020",
      "3 E1010",
      "4 S1010",
      "5 C1050",
      "6 C1040",
      "7 C1020",
    ]);
  });

  it("says why each activity is where it is, plainly", () => {
    const why = told.stops.map((s: Stop) => s.why);
    expect(why[0]).toBe("It can't finish until 2 days after E1020 Process piping finishes. Its start follows from that and its duration.");
    expect(why[1]).toBe("It can't start until 5 days after E1010 Install pumps starts.");
    expect(why[2]).toBe("It can't start until S1010 Erect clarifier walls finishes.");
    expect(why[6]).toBe("Its work can't start before the data date.");
    expect(told.stops[0]?.date).toBe("Finishes 08 Jul 26 17:00");
    expect(told.stops[6]?.date).toBe("Starts 23 Mar 26 08:00");
  });

  it("collects the relationships it runs along, for the Gantt", () => {
    expect(told.relationships.size).toBe(6);
  });
});

describe("plain words", () => {
  it("read dates in the format chosen, with the time where it's shown and asked for", () => {
    expect(readable("2026-06-25 08:00")).toBe("25 Jun 26 08:00");
    expect(readable("2026-06-25 08:00", false)).toBe("25 Jun 26");
    expect(readable(null)).toBe("no date");
    showDates({ style: "yyyy-mm-dd", time: false });
    expect(readable("2026-06-25 08:00")).toBe("2026-06-25");
    showDates(WITH_TIMES);
  });

  it("say days as people do", () => {
    expect(days(1)).toBe("1 day");
    expect(days(0.5)).toBe("half a day");
    expect(days(2)).toBe("2 days");
    expect(days(0.125)).toBe("0.13 days");
  });

  it("put each kind of cause in words, forward and backward", () => {
    const other = (id: number) => `A${id}`;
    const step = (field: string, cause: object) => ({ activity: 1, code: "A1", field, date: null, text: "", cause }) as never;
    expect(because(step("early_start", { kind: "relationship", from: 2, link: "FS", lag_hours: 16, lag_days: 2 }), other)).toBe(
      "It can't start until 2 days after A2 finishes.",
    );
    expect(because(step("early_start", { kind: "relationship", from: 2, link: "FS", lag_hours: -8, lag_days: -1 }), other)).toBe(
      "It can't start until 1 day before A2 finishes.",
    );
    expect(because(step("late_finish", { kind: "relationship", from: 3, link: "FS", lag_hours: 0 }), other)).toBe(
      "It must finish before A3 starts.",
    );
    expect(because(step("early_start", { kind: "constraint", constraint: "start-on-or-after", date: "2026-01-15 08:00" }), other)).toBe(
      "It can't start before 15 Jan 26 08:00.",
    );
    // Toward a successor, a negative lag lets it run past the successor's date.
    expect(
      because(step("late_finish", { kind: "relationship", from: 2, from_role: "successor", link: "FS", lag_hours: -1, lag_days: -0.125 }), other),
    ).toBe("It can finish no later than 0.13 days after A2 starts.");
    expect(
      because(step("late_finish", { kind: "relationship", from: 2, from_role: "successor", link: "FS", lag_hours: 8, lag_days: 1 }), other),
    ).toBe("It must finish 1 day before A2 starts.");
    // An early date pushed as late as possible is bounded by its successor.
    expect(
      because(
        step("early_start", { kind: "relationship", from: 2, from_role: "successor", as_late_as_possible: true, link: "FS", lag_hours: 8, lag_days: 1 }),
        other,
      ),
    ).toBe("Set as late as possible: It must finish 1 day before A2 starts.");
    expect(because(step("late_finish", { kind: "project-finish" }), other)).toBe(
      "Nothing after it holds it sooner, so the project's finish sets it.",
    );
  });
});
