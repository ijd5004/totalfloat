import { describe, expect, it } from "vitest";

import { sample } from "../edit/fixture";
import { type LineMode, type Logic, type Picture, barOf, drawBody } from "./draw";
import { ZOOMS, minutesOf, scaleOf, timesOf, xOf } from "./time";

const scale = scaleOf(minutesOf("2026-01-01 00:00"), ZOOMS[3] ?? { pxPerDay: 8, top: "month", bottom: "week" });
const x = (text: string) => xOf(scale, minutesOf(text));

describe("an activity's bar", () => {
  it("runs from early start to early finish when not started, with float to the late finish", () => {
    const rows = sample({ late_finish: ["2026-01-14 17:00", null, "2026-01-09 17:00"], critical: [false, false, true] });
    const times = timesOf(rows);
    expect(barOf(rows, times, 0, scale, undefined)).toEqual({
      kind: "task",
      critical: false,
      remaining: [x("2026-01-05 08:00"), x("2026-01-09 17:00")],
      float: [x("2026-01-09 17:00"), x("2026-01-14 17:00")],
    });
    // No late dates, or none later than the early finish: no float bar.
    expect(barOf(rows, times, 1, scale, undefined).float).toBeUndefined();
    expect(barOf(rows, times, 2, scale, undefined)).toMatchObject({ critical: true });
    expect(barOf(rows, times, 2, scale, undefined).float).toBeUndefined();
  });

  it("shows work done up to the data date, then what's left", () => {
    const rows = sample({
      status: ["in-progress", "completed", "not-started"],
      actual_start: ["2026-01-05 08:00", "2026-01-02 08:00", null],
      actual_finish: [null, "2026-01-03 17:00", null],
      early_start: ["2026-01-07 08:00", null, "2026-01-05 08:00"],
      early_finish: ["2026-01-09 17:00", null, "2026-01-09 17:00"],
    });
    const times = timesOf(rows);
    const dataDate = minutesOf("2026-01-07 08:00");
    expect(barOf(rows, times, 0, scale, dataDate)).toMatchObject({
      actual: [x("2026-01-05 08:00"), x("2026-01-07 08:00")],
      remaining: [x("2026-01-07 08:00"), x("2026-01-09 17:00")],
    });
    // Finished: all actual, nothing left and no float.
    expect(barOf(rows, times, 1, scale, dataDate)).toEqual({
      kind: "task",
      critical: false,
      actual: [x("2026-01-02 08:00"), x("2026-01-03 17:00")],
    });
  });

  it("draws milestones as a point: a start milestone at its start, a finish milestone at its finish", () => {
    const rows = sample({
      type: ["start-milestone", "finish-milestone", "finish-milestone"],
      actual_finish: [null, null, "2026-01-08 17:00"],
    });
    const times = timesOf(rows);
    expect(barOf(rows, times, 0, scale, undefined)).toEqual({ kind: "milestone", critical: false, at: x("2026-01-05 08:00") });
    expect(barOf(rows, times, 1, scale, undefined).at).toBe(x("2026-01-09 17:00"));
    expect(barOf(rows, times, 2, scale, undefined).at).toBe(x("2026-01-08 17:00"));
  });
});

describe("logic lines on the chart", () => {
  /** A canvas context that draws nothing and says what it was asked. */
  function recorder() {
    const dashes: number[][] = [];
    const ctx = new Proxy(
      { setLineDash: (d: number[]) => dashes.push(d), measureText: () => ({ width: 10 }) },
      { get: (target, key) => (key in target ? target[key as keyof typeof target] : () => {}), set: () => true },
    ) as unknown as CanvasRenderingContext2D;
    return { ctx, dashes };
  }
  const colours = {
    bg: "", band: "", chosen: "", line: "", text: "", muted: "", bar: "", critical: "", actual: "", float: "", summary: "", dataDate: "", today: "", font: "13px sans-serif", hivis: "", hivisInk: "", focus: "",
  };

  function picture(height: number, mode: LineMode = "all"): Picture & { logic: Logic } {
    // A band, then A1 to A3: A1 drives A2, and A2 links to A3 without driving.
    const rows = sample();
    const arranged = { lines: Int32Array.from([-1, 0, 1, 2]), bands: [{ path: "P", label: "P", title: "P", spoken: "WBS P", depth: 0, count: 3, collapsed: false, start: null, finish: null }], matched: 3 };
    const links = { count: 2, id: [1, 2], predecessor: [1, 2], successor: [2, 3], type: ["FS", "SS"], lag_hours: [0, 8], driving: [true, false] } as unknown as import("../api").Links;
    const lineOf = new Map([
      [1, 1],
      [2, 2],
      [3, 3],
    ]);
    return {
      rows,
      arranged,
      times: timesOf(rows),
      scale,
      top: 0,
      left: 0,
      width: 800,
      height,
      rowHeight: 22,
      active: 1,
      dataDates: new Map<number, number>(),
      today: null,
      grid: [],
      logic: { links, index: new Map([[1, [0]], [2, [0, 1]], [3, [1]]]), lineOf, mode, selected: 1, path: null },
    };
  }

  it("draws each relationship of the rows on screen once, driving solid and the rest dashed", () => {
    const { ctx, dashes } = recorder();
    const drawn = drawBody(ctx, colours, picture(400));
    expect(drawn.lines.map((l) => l.link)).toEqual([0, 1]);
    expect(dashes.slice(0, 4)).toEqual([[], [], [3, 3], []]);
    expect(drawn.arrows).toEqual([]);
  });

  it("ends a line whose other end is below the screen in an arrow leading to it", () => {
    const { ctx } = recorder();
    // Only two rows high: A3, on line 3, is off screen.
    const drawn = drawBody(ctx, colours, picture(60));
    expect(drawn.arrows).toHaveLength(1);
    expect(drawn.arrows[0]).toMatchObject({ to: 3 });
  });

  it("puts an arrow at each edge when both ends of a selected activity's line are off screen", () => {
    const { ctx } = recorder();
    // Scrolled so A1 is above and A3 below a one-row-high window on A2.
    const shown = { ...picture(22, "selected"), top: 44 };
    shown.logic = { ...shown.logic, selected: 2, index: new Map([[2, [0, 1]]]) };
    const drawn = drawBody(ctx, colours, shown);
    expect(drawn.arrows.map((a) => a.to).sort()).toEqual([1, 3]);
  });

  it("draws nothing for a line its index places on the wrong activity", () => {
    const { ctx } = recorder();
    const shown = picture(400);
    // An index left from another arrangement: A2 and A3 have swapped lines.
    shown.logic = { ...shown.logic, lineOf: new Map([[1, 1], [2, 3], [3, 2]]) };
    expect(drawBody(ctx, colours, shown).lines.map((l) => l.link)).toEqual([]);
  });

  it("weights only the relationships the engine's chain runs along as the path", () => {
    const { ctx } = recorder();
    const widths: number[] = [];
    const watched = new Proxy(ctx, {
      set: (_target, key, value) => {
        if (key === "lineWidth") widths.push(value as number);
        return true;
      },
      get: (target, key) => target[key as keyof CanvasRenderingContext2D],
    });
    const shown = picture(400);
    // All three on the path, which runs A1 to A2 only; A2 to A3 is beside it.
    shown.logic = { ...shown.logic, path: { ids: new Set([1, 2, 3]), relationships: new Set(), edges: new Set(["1>2"]) } };
    drawBody(watched, colours, shown);
    // One heavy line, and three outlined bars.
    expect(widths.filter((w) => w === 3)).toHaveLength(1 + 3);
  });

  it("weights only the relationship the chain names, of two parallel ones that both drive", () => {
    const { ctx } = recorder();
    const widths: number[] = [];
    const watched = new Proxy(ctx, {
      set: (_target, key, value) => {
        if (key === "lineWidth") widths.push(value as number);
        return true;
      },
      get: (target, key) => target[key as keyof CanvasRenderingContext2D],
    });
    const shown = picture(400, "all");
    // FS and SS+8h from A1 to A2 both drive; the chain names the FS, relationship 1.
    const links = { count: 2, id: [1, 2], predecessor: [1, 1], successor: [2, 2], type: ["FS", "SS"], lag_hours: [0, 8], driving: [true, true] } as unknown as import("../api").Links;
    shown.logic = { ...shown.logic, links, index: new Map([[1, [0, 1]], [2, [0, 1]]]), path: { ids: new Set([1, 2]), relationships: new Set([1]), edges: new Set() } };
    drawBody(watched, colours, shown);
    // One heavy line, and two outlined bars.
    expect(widths.filter((w) => w === 3)).toHaveLength(1 + 2);
  });

  it("outlines milestones on the path too", () => {
    const { ctx } = recorder();
    const widths: number[] = [];
    const watched = new Proxy(ctx, {
      set: (_target, key, value) => {
        if (key === "lineWidth") widths.push(value as number);
        return true;
      },
      get: (target, key) => target[key as keyof CanvasRenderingContext2D],
    });
    const shown = picture(400, "none");
    shown.rows = sample({ type: ["start-milestone", "finish-milestone", "task"] });
    shown.times = timesOf(shown.rows);
    shown.logic = { ...shown.logic, path: { ids: new Set([1, 2]), relationships: new Set(), edges: new Set() } };
    drawBody(watched, colours, shown);
    expect(widths.filter((w) => w === 3)).toHaveLength(2);
  });

  it("records each bar on screen, for clicking", () => {
    const { ctx } = recorder();
    const drawn = drawBody(ctx, colours, picture(400));
    expect(drawn.bars.map((b) => b.id)).toEqual([1, 2, 3]);
  });

  it("puts an arrow at the right edge for a successor that starts beyond it", () => {
    const { ctx } = recorder();
    // A2 starts in February, past the right edge of a 200-pixel chart.
    const shown = { ...picture(400), width: 200 };
    shown.rows = sample({ early_start: ["2026-01-05 08:00", "2026-02-02 08:00", "2026-02-02 08:00"], early_finish: ["2026-01-09 17:00", "2026-02-04 17:00", "2026-02-04 17:00"] });
    shown.times = timesOf(shown.rows);
    shown.logic = { ...shown.logic, mode: "selected", selected: 1, index: new Map([[1, [0]]]) };
    const drawn = drawBody(ctx, colours, shown);
    expect(drawn.arrows.map((a) => a.to)).toEqual([2]);
  });

  it("draws no arrow where a line between two bars on screen strays past the edge", () => {
    const { ctx } = recorder();
    // Both bars start 4 px from the left edge; an SS line between them runs out to the left of it.
    const shown = { ...picture(400), left: x("2026-01-05 08:00") - 4 };
    const links = { count: 1, id: [1], predecessor: [1], successor: [2], type: ["SS"], lag_hours: [0], driving: [true] } as unknown as import("../api").Links;
    shown.logic = { ...shown.logic, mode: "selected", selected: 1, links, index: new Map([[1, [0]], [2, [0]]]) };
    const drawn = drawBody(ctx, colours, shown);
    expect(drawn.lines).toHaveLength(1);
    expect(drawn.arrows).toEqual([]);
  });

  it("dims what's off a highlighted chain, and numbers the bars on it as the explain panel does", () => {
    const { ctx } = recorder();
    const alphas: number[] = [];
    const texts: string[] = [];
    const watched = new Proxy(ctx, {
      set: (_target, key, value) => {
        if (key === "globalAlpha") alphas.push(value as number);
        return true;
      },
      get: (target, key) =>
        key === "fillText" ? (text: string) => texts.push(text) : target[key as keyof CanvasRenderingContext2D],
    });
    const shown = picture(400, "none");
    // The chain runs A3 back to A1; A2 isn't on it.
    shown.logic = {
      ...shown.logic,
      path: { ids: new Set([1, 3]), relationships: new Set(), edges: new Set(), numbers: new Map([[3, 1], [1, 2]]), focus: null },
    };
    drawBody(watched, colours, shown);
    expect(alphas).toContain(0.28);
    expect(texts.filter((t) => t === "1" || t === "2").sort()).toEqual(["1", "2"]);
  });

  it("draws only the selected activity's lines, or none", () => {
    const { ctx } = recorder();
    expect(drawBody(ctx, colours, picture(400, "selected")).lines.map((l) => l.link)).toEqual([0]);
    expect(drawBody(ctx, colours, picture(400, "none")).lines).toEqual([]);
  });
});
