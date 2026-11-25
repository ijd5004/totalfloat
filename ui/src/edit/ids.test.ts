import { describe, expect, it } from "vitest";

import { nextId } from "./ids";

describe("the next Activity ID", () => {
  it("follows the band's prefix and step", () => {
    const band = ["RC1300", "RC1310", "RC1320", "RC1330", "RC1340", "RC1350"];
    expect(nextId(band, band)).toBe("RC1360");
    expect(nextId(["A1", "A2", "A3"], ["A1", "A2", "A3"])).toBe("A4");
  });

  it("keeps the number's width, and takes the step most neighbours have", () => {
    expect(nextId(["EX-0010", "EX-0020", "EX-0025", "EX-0035"], [])).toBe("EX-0045");
    expect(nextId(["A010", "A020"], [])).toBe("A030");
  });

  it("takes the prefix most of the band's IDs share", () => {
    expect(nextId(["CV100", "CV110", "MS1", "CV120"], [])).toBe("CV130");
  });

  it("passes over an ID taken anywhere in the schedule, case aside", () => {
    expect(nextId(["RC1340", "RC1350"], ["RC1340", "RC1350", "rc1360", "RC1370"])).toBe("RC1380");
  });

  it("uses the schedule's pattern for a band with none, and A1000 with none anywhere", () => {
    expect(nextId([], ["RC1000", "RC1010", "RC1020"])).toBe("RC1030");
    expect(nextId(["Kickoff"], [])).toBe("A1000");
    expect(nextId([], [])).toBe("A1000");
  });

  it("steps one ID alone by the schedule's step, or with none, by 10 when it's round, else by 1", () => {
    // A new node's first activity took A10 by the schedule's step of 1.
    expect(nextId(["A10"], ["A1", "A2", "A3", "A10"])).toBe("A11");
    // Not another prefix's step.
    expect(nextId(["A1000"], ["A1000", "B1", "B5", "B9"])).toBe("A1010");
    expect(nextId(["A1000"], [])).toBe("A1010");
    expect(nextId(["A7"], [])).toBe("A8");
  });

  it("works out a schedule-wide pattern over more IDs than a call takes arguments", () => {
    const all = Array.from({ length: 200_000 }, (_, k) => `A${(k + 1) * 10}`);
    expect(nextId([], all)).toBe("A2000010");
  });
});
