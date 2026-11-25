import { describe, expect, it } from "vitest";

import { ZOOMS } from "../gantt/time";
import { layoutOf, settingOf } from "./layouts";

const SETTING = {
  grid: {
    columns: [
      { field: "name" as const, width: 300.4 },
      { field: "before" as const, width: 200 },
      { field: "code" as const, width: 10 },
    ],
    sort: [{ column: "finish" as const, descending: true }],
    grouped: false,
    groupBy: null,
    text: "pour",
    quick: ["critical" as const],
    built: [],
  },
  zoom: 4,
  lines: "selected" as const,
  today: true,
  split: 0.5555,
};

describe("a layout", () => {
  it("keeps what's shown, in the file's terms", () => {
    expect(layoutOf(SETTING)).toEqual({
      schema: "totalfloat.layout/1",
      // Before and After aren't layout columns; widths are whole pixels in range.
      columns: [
        { field: "name", width: 300 },
        { field: "code", width: 40 },
      ],
      sort: [{ field: "finish", descending: true }],
      group: "none",
      filters: { text: "pour", quick: ["critical"] },
      gantt: { zoom: { px_per_day: 24, top: "month", bottom: "day" }, lines: "selected", today: true },
      split: 0.556,
    });
  });

  it("sets the app back as it was", () => {
    const back = settingOf(layoutOf(SETTING));
    expect(back.grid.columns.map((c) => c.field)).toEqual(["name", "code"]);
    expect(back).toMatchObject({ zoom: 4, lines: "selected", today: true, split: 0.556 });
    expect(back.grid).toMatchObject({ grouped: false, text: "pour", quick: ["critical"], sort: [{ column: "finish", descending: true }] });
  });

  it("keeps grouping by a code and a built filter, as query's parameters", () => {
    const grid = {
      ...SETTING.grid,
      grouped: true,
      groupBy: "code:Phase",
      built: [
        { field: "code" as const, value: "Phase=DESIGN" },
        { field: "critical" as const, value: "" },
        { field: "start_from" as const, value: "2026-01-05" },
      ],
    };
    const layout = layoutOf({ ...SETTING, grid });
    expect(layout.group).toBe("code:Phase");
    expect(layout.filters?.built).toEqual({ code: "Phase=DESIGN", critical: "true", start_from: "2026-01-05 00:00" });
    expect(settingOf(layout).grid).toMatchObject({ grouped: true, groupBy: "code:Phase", built: grid.built });
  });

  it("keeps any-of groups beside the built filter, and leaves both out when empty", () => {
    const built = [
      { field: "status" as const, value: "not-started" },
      { field: "critical" as const, value: "", group: 0 },
      { field: "float_max" as const, value: "8", group: 1 },
    ];
    const layout = layoutOf({ ...SETTING, grid: { ...SETTING.grid, built } });
    expect(layout.filters?.built).toEqual({ status: "not-started" });
    expect(layout.filters?.any).toEqual([{ critical: "true" }, { float_max: "8" }]);
    expect(settingOf(layout).grid.built).toEqual(built);
    // Groups alone: no built at all.
    const groupsOnly = layoutOf({ ...SETTING, grid: { ...SETTING.grid, built: built.slice(1) } });
    expect(groupsOnly.filters).not.toHaveProperty("built");
    expect(layoutOf(SETTING).filters).not.toHaveProperty("any");
  });

  it("keeps grouping by an activity field and a sort of three keys", () => {
    const sort = [
      { column: "wbs" as const, descending: false },
      { column: "float" as const, descending: true },
      { column: "code" as const, descending: false },
    ];
    const layout = layoutOf({ ...SETTING, grid: { ...SETTING.grid, grouped: true, groupBy: "start_month", sort } });
    expect(layout.group).toBe("start_month");
    expect(layout.sort).toEqual([
      { field: "wbs", descending: false },
      { field: "float", descending: true },
      { field: "code", descending: false },
    ]);
    expect(settingOf(layout).grid).toMatchObject({ grouped: true, groupBy: "start_month", sort });
  });

  it("keeps further levels of grouping in order, and leaves them out when there are none", () => {
    const grid = { ...SETTING.grid, grouped: true, groupBy: null, thenBy: ["code:Phase", "status"] };
    const layout = layoutOf({ ...SETTING, grid });
    expect(layout.group).toBe("wbs");
    expect(layout.then_by).toEqual(["code:Phase", "status"]);
    expect(settingOf(layout).grid).toMatchObject({ grouped: true, groupBy: null, thenBy: ["code:Phase", "status"] });
    expect(layoutOf({ ...SETTING, grid: { ...grid, thenBy: [] } })).not.toHaveProperty("then_by");
    expect(layoutOf({ ...SETTING, grid: { ...grid, grouped: false } })).not.toHaveProperty("then_by");
    expect(settingOf({ ...layoutOf(SETTING), group: "wbs" }).grid).not.toHaveProperty("thenBy");
  });

  it("opens an old layout, grouped by the WBS with no built filter, as before", () => {
    const old = { ...layoutOf(SETTING), group: "wbs" };
    expect(old.filters).not.toHaveProperty("built");
    expect(settingOf(old).grid).toMatchObject({ grouped: true, groupBy: null, built: [] });
    expect(settingOf({ ...old, group: "none" }).grid).toMatchObject({ grouped: false, groupBy: null });
  });

  it("keeps a fitted Gantt as fit, both ways", () => {
    const fitted = layoutOf({ ...SETTING, zoom: "fit" });
    expect(fitted.gantt.zoom).toBe("fit");
    expect(settingOf(fitted).zoom).toBe("fit");
  });

  it("takes the nearest zoom to a scale it doesn't have", () => {
    const layout = layoutOf(SETTING);
    expect(settingOf({ ...layout, gantt: { ...layout.gantt, zoom: { px_per_day: 7, top: "month", bottom: "week" } } }).zoom).toBe(
      ZOOMS.findIndex((z) => z.pxPerDay === 8),
    );
  });
});
