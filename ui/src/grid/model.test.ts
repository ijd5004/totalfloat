import { describe, expect, it } from "vitest";

import {
  DEFAULT_ARRANGEMENT,
  NO_VALUE,
  type Arrangement,
  type Grouping,
  type Model,
  arrange,
  bandOf,
  buildModel,
  filtered,
  natural,
  sorted,
  wbsNames,
  wbsOrder,
  wbsRefs,
} from "./model";
import type { WbsNode } from "../api";
import { sample } from "../edit/fixture";
import { AT_SCALE, SCALE, synthetic, with_ } from "./model.fixture";

const ROWS = synthetic(SCALE);
const BIG = buildModel(ROWS);

// The reference task: grouping and sorting 50,000 strings with plain
// JavaScript, the same kind of work as the grid's (string keys, maps, small
// objects, a sort with a comparator), so load slows both alike. It takes about
// 40 ms on the development machine when it's quiet.
const KEYS = Array.from({ length: SCALE }, (_, i) => `K${(i * 7919) % SCALE}`);
let sink = 0;
function reference(): number {
  const groups = new Map<string, { key: string; members: number[] }>();
  for (let i = 0; i < KEYS.length; i++) {
    const key = `${(KEYS[i] ?? "").slice(0, 4)}/${i % 7}`;
    let group = groups.get(key);
    if (!group) groups.set(key, (group = { key, members: [] }));
    group.members.push(i);
  }
  const order = Array.from(KEYS.keys()).sort((a, b) => {
    const x = KEYS[a] ?? "";
    const y = KEYS[b] ?? "";
    return x < y ? -1 : x > y ? 1 : 0;
  });
  return groups.size + (order[0] ?? 0);
}

/**
 * How long `work` takes, in reference tasks. Seven times over, on a
 * model built afresh outside the timing (so nothing an earlier run worked
 * out, such as sort ranks, is reused), the reference task and `work` run one
 * straight after the other, in turns which goes first. The answer is the
 * median of the seven ratios. Load from other processes slows both halves of
 * a pair alike, so it cancels; a slower `work` raises every ratio. Wall-clock
 * budgets, which only hold on a quiet machine, are in `model.budget.ts`.
 */
function timed<T>(work: (model: Model) => T): [T, number] {
  const ratios: number[] = [];
  let result: T | undefined;
  for (let run = 0; run < 7; run++) {
    const model = buildModel(ROWS);
    let took = 0;
    let referenceTook = 0;
    for (const isWork of [run % 2 === 0, run % 2 !== 0]) {
      const start = performance.now();
      if (isWork) result = work(model);
      else sink += reference();
      const ms = performance.now() - start;
      if (isWork) took = ms;
      else referenceTook = ms;
    }
    ratios.push(took / referenceTook);
  }
  ratios.sort((a, b) => a - b);
  expect(sink).toBeGreaterThan(0);
  return [result as T, ratios[3] ?? Infinity];
}

// Each limit is about two and a half times what the quiet development
// machine measures, and stays clear of the most measured under a load
// average of 27 on its four cores, so a fivefold slowdown fails and load
// doesn't. Under load the runs take several times longer, hence the timeout.
describe("on 50,000 activities", { timeout: 120_000 }, () => {
  it("filters in under 0.08 reference tasks", () => {
    const [found, ratio] = timed(AT_SCALE.filter);
    expect(ratio).toBeLessThan(0.08);
    expect(found.length).toBe(Math.ceil(50_000 / 15));
    for (const i of found) {
      expect(BIG.rows.critical[i]).toBe(true);
      expect(BIG.search[i]).toContain("pour");
    }
  });

  it("sorts by two keys in under 3 reference tasks", () => {
    const [order, ratio] = timed(AT_SCALE.sortTwoKeys);
    expect(ratio).toBeLessThan(3);
    const float = BIG.rows.total_float_hours;
    for (let k = 1; k < order.length; k++) {
      const a = float[order[k - 1] ?? 0] ?? null;
      const b = float[order[k] ?? 0] ?? null;
      if (a !== null && b !== null) expect(a).toBeLessThanOrEqual(b);
      if (a === null) expect(b).toBeNull();
    }
  });

  it("groups into WBS bands in under 3 reference tasks", () => {
    const [arranged, ratio] = timed(AT_SCALE.wbs);
    expect(ratio).toBeLessThan(3);
    expect(arranged.matched).toBe(50_000);
    // PLANT, 10 areas, and under each the units that occur there.
    expect(arranged.bands[0]).toMatchObject({ path: "PLANT", depth: 0, count: 50_000 });
    expect(arranged.bands.filter((b) => b.depth === 1)).toHaveLength(10);
    expect(arranged.lines.length).toBe(50_000 + arranged.bands.length);
  });

  it("groups by an activity code in under 3 reference tasks", () => {
    // Twelve phases; every seventh activity holds none.
    const [arranged, ratio] = timed(AT_SCALE.phase);
    expect(ratio).toBeLessThan(3);
    expect(arranged.matched).toBe(50_000);
    expect(arranged.bands.map((b) => b.label)).toEqual([
      ...Array.from({ length: 12 }, (_, k) => `PH${k + 1}`),
      NO_VALUE,
    ]);
    expect(arranged.bands.reduce((sum, b) => sum + b.count, 0)).toBe(50_000);
    expect(arranged.bands.at(-1)?.count).toBe(Math.ceil(50_000 / 7));
    expect(arranged.lines.length).toBe(50_000 + 13);
  });

  it("groups by the WBS, then an activity code, in under 4 reference tasks", () => {
    const [arranged, ratio] = timed(AT_SCALE.wbsThenPhase);
    expect(ratio).toBeLessThan(4);
    expect(arranged.matched).toBe(50_000);
    const phases = arranged.bands.filter((b) => b.path.includes("\u0001"));
    expect(phases.reduce((sum, b) => sum + b.count, 0)).toBe(50_000);
    expect(arranged.lines.length).toBe(50_000 + arranged.bands.length);
    // And the other way round: phases, then the WBS within each.
    const [turned, turnedRatio] = timed(AT_SCALE.phaseThenWbs);
    expect(turnedRatio).toBeLessThan(4);
    expect(turned.bands.filter((b) => b.depth === 0)).toHaveLength(13);
    expect(turned.lines.length).toBe(50_000 + turned.bands.length);
  });

  it("groups by two fields of 20,000 values each in under 5 reference tasks, or one and the WBS in under 12", () => {
    const [arranged, ratio] = timed(AT_SCALE.roomThenTag);
    expect(ratio).toBeLessThan(5);
    expect(arranged.bands.filter((b) => b.depth === 0)).toHaveLength(20_000);
    expect([...arranged.lines].filter((line) => line >= 0)).toHaveLength(50_000);
    // And the WBS within each of the 20,000 bands.
    const [wbs, wbsRatio] = timed(AT_SCALE.roomThenWbs);
    expect(wbsRatio).toBeLessThan(12);
    expect([...wbs.lines].filter((line) => line >= 0)).toHaveLength(50_000);
  });
});

describe("grouping by a field", () => {
  const rows = synthetic(6);
  const model = buildModel(rows);
  const by = (values: [number, string | number][], order?: [string, number][]): Grouping => ({
    field: "code:Phase",
    title: "Phase",
    values: new Map(values),
    ...(order ? { order: new Map(order) } : {}),
  });
  const labels = (arrangement: Arrangement) => {
    const arranged = arrange(model, arrangement);
    return [...arranged.lines].map((line) => (line < 0 ? `[${arranged.bands[-line - 1]?.title}]` : rows.id[line]));
  };

  it("bands each value, in natural order, with no value last", () => {
    const grouping = by([
      [1, "B10"],
      [2, "B9"],
      [3, "B10"],
      [5, "B9"],
    ]);
    expect(labels(with_({ by: grouping }))).toEqual([
      "[Phase: B9]",
      2,
      5,
      "[Phase: B10]",
      1,
      3,
      `[Phase: ${NO_VALUE}]`,
      4,
      6,
    ]);
  });

  it("follows the code type's own sequence where it's given", () => {
    const grouping = by(
      [
        [1, "DESIGN"],
        [2, "BUILD"],
        [3, "ALPHA"],
      ],
      [
        ["DESIGN", 0],
        ["BUILD", 1],
      ],
    );
    const arranged = arrange(model, with_({ by: grouping }));
    expect(arranged.bands.map((b) => b.label)).toEqual(["DESIGN", "BUILD", "ALPHA", NO_VALUE]);
  });

  it("orders numbers by value, and collapses a band by its path", () => {
    const grouping = by([
      [1, 10],
      [2, 9.5],
      [3, 10],
    ]);
    const open = arrange(model, with_({ by: grouping }));
    expect(open.bands.map((b) => b.label)).toEqual(["9.5", "10", NO_VALUE]);
    const ten = open.bands[1]?.path ?? "";
    expect(labels(with_({ by: grouping, collapsed: new Set([ten]) }))).toEqual([
      "[Phase: 9.5]",
      2,
      "[Phase: 10]",
      `[Phase: ${NO_VALUE}]`,
      4,
      5,
      6,
    ]);
    // A WBS path can't collapse a field's band, nor the other way round.
    expect(open.bands.every((b) => !b.path.includes(".") || b.path.includes("\u0000"))).toBe(true);
  });

  it("keeps only what a built filter matched, with the other filters", () => {
    const matching = new Set([2, 3, 4]);
    expect(labels(with_({ grouped: false, matching }))).toEqual([2, 3, 4]);
    expect(labels(with_({ grouped: false, matching, quick: new Set(["critical"]) }))).toEqual([]);
    expect(labels(with_({ grouped: false, matching, only: new Set([3, 4, 5]) }))).toEqual([3, 4]);
  });

  it("bands only matching activities, and leaves out empty bands", () => {
    const grouping = by([
      [1, "A"],
      [2, "B"],
    ]);
    expect(labels(with_({ by: grouping, matching: new Set([2, 3]) }))).toEqual([
      "[Phase: B]",
      2,
      `[Phase: ${NO_VALUE}]`,
      3,
    ]);
  });
});

describe("grouping by several levels", () => {
  const rows = synthetic(30);
  const model = buildModel(rows);
  // Phase A for even task_ids, B for odd ones up to 20; none after.
  const values = new Map<number, string>();
  for (let id = 1; id <= 20; id++) values.set(id, id % 2 ? "B" : "A");
  const phase: Grouping = { field: "code:Phase", title: "Phase", values };
  const status: Grouping = { field: "status", title: "Status", values: new Map(rows.id.map((id, i) => [id, rows.status[i] ?? ""])) };

  it("nests each level's bands in the bands of the level before, with counts", () => {
    const arranged = arrange(model, with_({ by: phase, then: [status] }));
    const top = arranged.bands.filter((b) => b.depth === 0);
    expect(top.map((b) => [b.label, b.count])).toEqual([
      ["A", 10],
      ["B", 10],
      [NO_VALUE, 10],
    ]);
    // Each phase band is followed by its status bands, which hold its activities.
    const lines = [...arranged.lines];
    const a = top[0];
    const at = lines.findIndex((_, k) => bandOf(arranged, k) === a);
    const inner = bandOf(arranged, at + 1);
    expect(inner?.depth).toBe(1);
    expect(inner?.path.startsWith(`${a?.path}\u0001status\u0000`)).toBe(true);
    const under = arranged.bands.filter((b) => b.path.startsWith(`${a?.path}\u0001`));
    expect(under.reduce((sum, b) => sum + b.count, 0)).toBe(10);
    expect(lines.filter((line) => line >= 0)).toHaveLength(30);
    // The same status band under two phases is two bands, each its own.
    expect(new Set(arranged.bands.map((b) => b.path)).size).toBe(arranged.bands.length);
  });

  it("collapses an inner band alone, and an outer band with everything in it", () => {
    const open = arrange(model, with_({ by: phase, then: [status] }));
    const inner = open.bands.find((b) => b.depth === 1);
    const outer = open.bands.find((b) => b.depth === 0);
    if (!inner || !outer) throw new Error("no bands");
    const innerShut = arrange(model, with_({ by: phase, then: [status], collapsed: new Set([inner.path]) }));
    expect(innerShut.lines.length).toBe(open.lines.length - inner.count);
    expect(innerShut.bands).toHaveLength(open.bands.length);
    const outerShut = arrange(model, with_({ by: phase, then: [status], collapsed: new Set([outer.path]) }));
    const inside = open.bands.filter((b) => b.path.startsWith(`${outer.path}\u0001`)).length;
    expect(outerShut.lines.length).toBe(open.lines.length - outer.count - inside);
    // An inner band's path doesn't collapse the same value under another band.
    const twin = open.bands.find((b) => b.depth === 1 && b.label === inner.label && b !== inner);
    expect(innerShut.bands.find((b) => b.path === twin?.path)?.collapsed).toBe(false);
  });

  it("splits a WBS node's own activities by the next level, after its child bands", () => {
    const arranged = arrange(model, with_({ then: [phase] }));
    const titles = [...arranged.lines].map((line, k) => (line < 0 ? bandOf(arranged, k)?.title : rows.id[line]));
    // PLANT.AREA0.UNIT0 holds task_id 1 (index 0), which has phase B.
    const unit = titles.indexOf("PLANT.AREA0.UNIT0");
    expect(titles.slice(unit + 1, unit + 3)).toEqual(["Phase: B", 1]);
    const band = arranged.bands.find((b) => b.title === "PLANT.AREA0.UNIT0");
    expect(arranged.bands.find((b) => b.path === `${band?.path}\u0001code:Phase\u0000B`)?.depth).toBe((band?.depth ?? 0) + 1);
    // Every activity once, under a phase band.
    expect([...arranged.lines].filter((line) => line >= 0)).toHaveLength(30);
  });

  it("puts WBS bands within a field's bands, a level deeper", () => {
    const arranged = arrange(model, with_({ by: phase, then: [null] }));
    const a = arranged.bands.find((b) => b.depth === 0 && b.label === "A");
    const plant = arranged.bands.find((b) => b.path === `${a?.path}\u0001PLANT`);
    expect(plant).toMatchObject({ depth: 1, count: 10, title: "PLANT" });
  });
});

describe("the arrangement", () => {
  const small = buildModel(synthetic(30));

  it("orders bands by the WBS's own sequence, then code", () => {
    const rows = { ...synthetic(4), wbs_path: ["P.CIV", "P.PRE", "P.STR.STEEL", "P.STR.CONC"] };
    // The server's tree, in WBS sequence: PRE, CIV, STR; under STR, CONC before STEEL.
    const order = wbsOrder([
      {
        project: 1,
        code: "P",
        nodes: [
          {
            id: 1,
            code: "P",
            name: "Plant",
            children: [
              { id: 2, code: "PRE", name: "Pre", children: [] },
              { id: 3, code: "CIV", name: "Civil", children: [] },
              {
                id: 4,
                code: "STR",
                name: "Structures",
                children: [
                  { id: 5, code: "CONC", name: "Concrete", children: [] },
                  { id: 6, code: "STEEL", name: "Steel", children: [] },
                ],
              },
            ],
          },
        ],
      },
    ]);
    const bands = (o?: Map<string, number>) => arrange(buildModel(rows), DEFAULT_ARRANGEMENT, o).bands.map((b) => b.path);
    expect(bands(order)).toEqual(["P", "P.PRE", "P.CIV", "P.STR", "P.STR.CONC", "P.STR.STEEL"]);
    // Without the tree, codes in natural order.
    expect(bands()).toEqual(["P", "P.CIV", "P.PRE", "P.STR", "P.STR.CONC", "P.STR.STEEL"]);
    // A node the tree doesn't know goes after those it does.
    const partial = new Map([["P", 0], ["P.STR", 1]]);
    expect(bands(partial).slice(0, 4)).toEqual(["P", "P.STR", "P.STR.CONC", "P.STR.STEEL"]);
  });

  it("names each WBS band, with its path and count after the name, and says the name to screen readers", () => {
    const rows = { ...synthetic(4), wbs_path: ["EC.1", "EC.1.1", "EC.2", "EC.3"] };
    const names = wbsNames([
      {
        project: 1,
        code: "EC",
        nodes: [
          {
            id: 1,
            code: "EC",
            name: "Effluent clarifier",
            children: [
              { id: 2, code: "1", name: "Civil", children: [{ id: 3, code: "1", name: "Excavation", children: [] }] },
              { id: 4, code: "2", name: "", children: [] },
              { id: 5, code: "3", name: "EC.3", children: [] },
            ],
          },
        ],
      },
    ]);
    expect([...names]).toEqual([
      ["EC", "Effluent clarifier"],
      ["EC.1", "Civil"],
      ["EC.1.1", "Excavation"],
      ["EC.3", "EC.3"],
    ]);
    const bands = arrange(buildModel(rows), DEFAULT_ARRANGEMENT, undefined, names).bands;
    expect(bands.map((b) => [b.name, b.title, b.count])).toEqual([
      ["Effluent clarifier", "EC", 4],
      ["Civil", "EC.1", 2],
      ["Excavation", "EC.1.1", 1],
      // A node with no name, or a name that's its path, shows its path once, as before.
      [undefined, "EC.2", 1],
      [undefined, "EC.3", 1],
    ]);
    expect(bands[1]?.spoken).toBe("WBS Civil, EC.1");
    expect(bands[3]?.spoken).toBe("WBS EC.2");
  });

  it("spans each band from its earliest start to its latest finish, actual first", () => {
    const rows = {
      ...synthetic(3),
      wbs_path: ["P.A", "P.A", "P.B"],
      early_start: ["2026-02-01 08:00", "2026-01-10 08:00", "2026-03-01 08:00"],
      early_finish: ["2026-02-05 17:00", "2026-01-20 17:00", "2026-04-01 17:00"],
      actual_start: [null, "2026-01-05 08:00", null],
      actual_finish: [null, null, null],
    };
    const arranged = arrange(buildModel(rows), DEFAULT_ARRANGEMENT);
    const span = (path: string) => arranged.bands.find((b) => b.path === path);
    expect(span("P.A")).toMatchObject({ start: "2026-01-05 08:00", finish: "2026-02-05 17:00" });
    expect(span("P.B")).toMatchObject({ start: "2026-03-01 08:00", finish: "2026-04-01 17:00" });
    expect(span("P")).toMatchObject({ start: "2026-01-05 08:00", finish: "2026-04-01 17:00" });
  });

  it("hides what's under a collapsed band, and counts it", () => {
    const open = arrange(small, with_({}));
    const area = open.bands.find((b) => b.path === "PLANT.AREA1");
    expect(area?.count).toBe(3);
    const shut = arrange(small, with_({ collapsed: new Set(["PLANT.AREA1"]) }));
    expect(shut.lines.length).toBe(open.lines.length - 3 - open.bands.filter((b) => b.path.startsWith("PLANT.AREA1.")).length);
    const line = [...shut.lines].findIndex((v, k) => bandOf(shut, k)?.path === "PLANT.AREA1" && v < 0);
    expect(bandOf(shut, line)?.collapsed).toBe(true);
  });

  it("widens by status and narrows by critical", () => {
    const open = filtered(small, with_({ quick: new Set(["not-started", "in-progress"]) }));
    expect(open.length).toBe(20);
    const criticalOpen = filtered(small, with_({ quick: new Set(["critical", "not-started", "in-progress"]) }));
    expect([...criticalOpen].every((i) => small.rows.critical[i] && small.rows.status[i] !== "completed")).toBe(true);
    expect(criticalOpen.length).toBeGreaterThan(0);
  });

  it("lists rows flat when not grouped", () => {
    const flat = arrange(small, with_({ grouped: false }));
    expect(flat.bands).toHaveLength(0);
    expect([...flat.lines]).toEqual(Array.from({ length: 30 }, (_, i) => i));
  });

  it("keeps missing values last whichever way a column sorts", () => {
    for (const descending of [false, true]) {
      const order = sorted(small, filtered(small, with_({})), [{ column: "float", descending }]);
      const last = small.rows.total_float_hours[order[order.length - 1] ?? 0];
      expect(last).toBeNull();
    }
  });
});

describe("natural order", () => {
  it("sorts the grid as it sorts the bands", () => {
    const codes = [
      "A10", "a2", "A1", "B", "A01", "A2", "B-2", "B10", "pump 3", "pump 12", "Pump 3",
      "A281583087285900010", "A281583087285900001", "A99999999999999999999", "A1000000000000000",
    ];
    const rows = synthetic(codes.length);
    rows.code.splice(0, codes.length, ...codes);
    const model = buildModel(rows);
    const order = sorted(model, filtered(model, with_({})), [{ column: "code", descending: false }]);
    expect([...order].map((i) => codes[i])).toEqual([...codes].sort(natural));
  });

  it("reads digits by value and ignores case until a tie", () => {
    const codes = ["A10", "a2", "A1", "B", "A01", "A2"];
    expect([...codes].sort(natural)).toEqual(["A01", "A1", "A2", "a2", "A10", "B"]);
    expect(["B-2", "B", "B10", "B2"].sort(natural)).toEqual(["B", "B2", "B10", "B-2"]);
    // Past what a number holds exactly, digits still order by value.
    expect(["A281583087285900010", "A281583087285900001", "A9"].sort(natural)).toEqual([
      "A9",
      "A281583087285900001",
      "A281583087285900010",
    ]);
  });
});

describe("building the schedule", () => {
  const rows = sample({ wbs_path: ["P", "P", "Q.R"] });
  const building = { ...DEFAULT_ARRANGEMENT, grouped: true, building: new Set(["P", "Q", "Q.R", "S"]) };
  const lines = (arranged: ReturnType<typeof arrange>) =>
    [...arranged.lines].map((value) => {
      const band = value < 0 ? arranged.bands[-value - 1] : undefined;
      if (!band) return rows.code[value];
      return band.adds !== undefined ? `+${band.adds}` : band.path;
    });

  it("gives every WBS node a band, empty ones too, each ending with an Add activity line", () => {
    const arranged = arrange(buildModel(rows), building, new Map([["P", 0], ["Q", 1], ["Q.R", 2], ["S", 3]]));
    expect(lines(arranged)).toEqual(["P", "A1", "A2", "+P", "Q", "Q.R", "A3", "+Q.R", "+Q", "S", "+S"]);
    expect(arranged.bands.find((band) => band.adds === "S")?.spoken).toBe("Add an activity to WBS S");
  });

  it("gives a line to a new node being typed, after its parent's child nodes", () => {
    const arranged = arrange(buildModel(rows), { ...building, addingWbs: "Q" }, new Map([["P", 0], ["Q", 1], ["Q.R", 2], ["S", 3]]));
    const shown = [...arranged.lines].map((value) => {
      const band = value < 0 ? arranged.bands[-value - 1] : undefined;
      return band?.newWbs !== undefined ? `new:${band.newWbs}` : (lines(arranged)[[...arranged.lines].indexOf(value)] ?? "");
    });
    expect(shown).toEqual(["P", "A1", "A2", "+P", "Q", "Q.R", "A3", "+Q.R", "new:Q", "+Q", "S", "+S"]);
  });

  it("leaves out empty nodes while a filter narrows the rows, keeping the Add activity lines", () => {
    const arranged = arrange(buildModel(rows), { ...building, text: "A3" });
    expect(lines(arranged)).toEqual(["Q", "Q.R", "A3", "+Q.R", "+Q"]);
  });
});

describe("how an edit names a WBS node", () => {
  const node = (id: number, code: string, children: WbsNode[] = []): WbsNode => ({ id, code, name: "", children });

  it("by its code where it's the only one, else by its ID, and not at all when another node has the ID as a code", () => {
    const refs = wbsRefs([
      // 1's code repeats as 1.1's, and its ID 2 is 2's code; 1.1's ID 7 is no node's code.
      { project: 5, code: "P", nodes: [node(2, "1", [node(7, "1")]), node(1, "2", [node(8, "x"), node(9, "X")])] },
    ]);
    expect(refs.get("1")).toEqual({ project: 5, code: "1", id: 2, ref: null });
    expect(refs.get("1.1")).toEqual({ project: 5, code: "1", id: 7, ref: "7" });
    expect(refs.get("2")).toEqual({ project: 5, code: "2", id: 1, ref: "2" });
    // The server matches codes in any case.
    expect(refs.get("2.x")).toEqual({ project: 5, code: "x", id: 8, ref: "8" });
    // The project's own node is the top one with the project's code.
    expect(wbsRefs([{ project: 5, code: "P", nodes: [node(1, "P", [node(2, "A")])] }]).get("P")?.projectNode).toBe(true);
  });

  it("counts codes within each project, and gives each node's project", () => {
    const refs = wbsRefs([
      { project: 5, code: "P", nodes: [node(3, "A")] },
      { project: 6, code: "Q", nodes: [node(4, "B", [node(10, "A")])] },
    ]);
    expect(refs.get("A")).toMatchObject({ project: 5, ref: "A" });
    expect(refs.get("B.A")).toMatchObject({ project: 6, ref: "A" });
  });
});
