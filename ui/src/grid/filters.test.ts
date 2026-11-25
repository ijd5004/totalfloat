import { describe as group, expect, it } from "vitest";

import { builtOf, codeParts, complete, conditionsOf, describe, listItem, paramsOf, unused } from "./filters";

group("built filters", () => {
  it("become query's parameters, whole days for dates", () => {
    expect(
      paramsOf([
        { field: "code", value: "Phase=DESIGN" },
        { field: "float_max", value: " 40 " },
        { field: "critical", value: "" },
        { field: "start_from", value: "2026-01-12" },
        { field: "finish_to", value: "2026-02-01" },
      ]),
    ).toEqual({
      code: "Phase=DESIGN",
      float_max: "40",
      critical: "true",
      start_from: "2026-01-12 00:00",
      finish_to: "2026-02-01 23:59",
    });
  });

  it("leave out conditions without a usable value", () => {
    expect(complete({ field: "wbs", value: "  " })).toBe(false);
    expect(complete({ field: "float_min", value: "lots" })).toBe(false);
    expect(complete({ field: "start_to", value: "13/01/2026" })).toBe(false);
    expect(paramsOf([{ field: "wbs", value: "" }, { field: "text", value: "pump" }])).toEqual({ text: "pump" });
  });

  it("read as a summary, and offer each filter once", () => {
    const conditions = [
      { field: "code" as const, value: "Phase=DESIGN" },
      { field: "float_max" as const, value: "40" },
      { field: "wbs" as const, value: "" },
    ];
    expect(describe(conditions)).toBe("Phase=DESIGN, float ≤ 40h");
    const left = unused(conditions).map((c) => c.id);
    expect(left).not.toContain("code");
    expect(left).not.toContain("wbs");
    expect(left).toContain("float_min");
  });

  it("need a code's type and value, and escape a list item's commas", () => {
    expect(complete({ field: "code", value: "Phase=" })).toBe(false);
    expect(complete({ field: "code", value: "Phase=A=B" })).toBe(true);
    expect(codeParts("Phase=A=B")).toEqual({ type: "Phase", value: "A=B" });
    expect(listItem("udf:Cost, phase\\2")).toBe("udf:Cost\\, phase\\\\2");
  });
});

group("any-of groups", () => {
  const conditions = [
    { field: "status" as const, value: "not-started" },
    { field: "critical" as const, value: "", group: 0 },
    { field: "float_max" as const, value: "8", group: 3 },
    { field: "text" as const, value: "", group: 5 },
  ];

  it("become query's any, as JSON, leaving out groups with nothing complete", () => {
    expect(builtOf(conditions)).toEqual({ built: { status: "not-started" }, any: [{ critical: "true" }, { float_max: "8" }] });
    expect(paramsOf(conditions)).toEqual({
      status: "not-started",
      any: JSON.stringify([{ critical: "true" }, { float_max: "8" }]),
    });
    expect(paramsOf(conditions.slice(0, 1))).toEqual({ status: "not-started" });
  });

  it("come back from a layout's or named filter's built and any, numbered from 0", () => {
    expect(conditionsOf({ status: "not-started" }, [{ critical: "true" }, { float_max: "8" }])).toEqual([
      { field: "status", value: "not-started" },
      { field: "critical", value: "", group: 0 },
      { field: "float_max", value: "8", group: 1 },
    ]);
  });

  it("read as any of (…; …), and each group offers what it doesn't hold", () => {
    expect(describe(conditions)).toBe("not-started, any of (critical; float ≤ 8h)");
    expect(unused(conditions, 0).map((c) => c.id)).not.toContain("critical");
    expect(unused(conditions).map((c) => c.id)).toContain("critical");
  });
});
