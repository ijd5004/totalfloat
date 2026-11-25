import { describe, expect, it } from "vitest";

import type { Links, Summary } from "../api";
import { golden } from "../api/schemas/golden";
import { sample } from "../edit/fixture";
import { lagDay, linkText } from "./Inspector";
import { logicOf } from "./links";

const LINKS: Links = {
  schema: "totalfloat.links/1",
  count: 3,
  id: [11, 12, 13],
  predecessor: [1, 2, 3],
  successor: [3, 3, 4],
  type: ["FS", "SS", "FF"],
  lag_hours: [0, -16, 8],
  driving: [false, true, false],
};

describe("an activity's logic", () => {
  it("lists its predecessors and successors from the links, driving first", () => {
    const { predecessors, successors } = logicOf(LINKS, 3);
    expect(predecessors.map((l) => [l.link, l.other, l.type, l.lagHours, l.driving])).toEqual([
      [12, 2, "SS", -16, true],
      [11, 1, "FS", 0, false],
    ]);
    expect(successors.map((l) => [l.link, l.other])).toEqual([[13, 4]]);
    expect(logicOf(LINKS, 99)).toEqual({ predecessors: [], successors: [] });
  });

  it("writes a link's type with its lag, signed, in days where the day is known", () => {
    const [ss, fs] = logicOf(LINKS, 3).predecessors;
    expect(ss && linkText(ss, 8)).toBe("SS −2d");
    expect(fs && linkText(fs, 8)).toBe("FS");
    const [ff] = logicOf(LINKS, 3).successors;
    expect(ff && linkText(ff, null)).toBe("FF +8h");
  });
});

describe("the day a lag is counted in", () => {
  const rows = sample({ hours_per_day: [16, 6, 8] });
  const summary = (lag: string): Summary => {
    const base = golden["summary-1"];
    const [first] = base.inspect.projects;
    if (!first) throw new Error("the golden summary has no project");
    return { ...base, inspect: { ...base.inspect, projects: [{ ...first, id: 7, options: { ...first.options, lag_calendar: lag } }] } };
  };

  it("follows the project's lag calendar option, as the engine counts it", () => {
    expect(lagDay(rows, 1, 2, summary("predecessor"))).toBe(16);
    expect(lagDay(rows, 1, 2, summary("successor"))).toBe(6);
    expect(lagDay(rows, 1, 2, summary("24-hour"))).toBe(24);
    // The default calendar's day isn't in the rows: hours instead.
    expect(lagDay(rows, 1, 2, summary("project-default"))).toBeNull();
    // Without the summary, the default: the predecessor's.
    expect(lagDay(rows, 1, 2, null)).toBe(16);
  });
});
