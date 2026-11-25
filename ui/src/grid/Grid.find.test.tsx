// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";

import { ApiError, type QueryParams, type QueryReport, type Rows } from "../api";
import { Grid, type GridFinder, type GridLayout } from "./Grid";

afterEach(cleanup);

const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
});
afterAll(() => {
  if (height) Object.defineProperty(HTMLElement.prototype, "clientHeight", height);
});

/** Opens the filter builder: Add filter, then More conditions. */
function openBuilder() {
  fireEvent.click(screen.getByRole("button", { name: "Add filter" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "More conditions…" }));
}

/** The filters applied, as their chips read. */
function chips(): string[] {
  return screen.queryAllByRole("listitem").filter((li) => li.closest('[aria-label="Filters applied"]')).map((li) => li.textContent ?? "");
}

/** A grouping control, opening the grouping panel for it if it's closed. */
function grouping(label: string): HTMLElement {
  if (!screen.queryByLabelText(label) && !screen.queryByRole("dialog", { name: /^Grouping:/ })) {
    fireEvent.click(screen.getByRole("button", { name: /^Grouping:/ }));
  }
  return screen.getByLabelText(label);
}

const CODES = ["A1", "A2", "A3", "A4"];
const PHASE: Record<number, string> = { 1: "BUILD", 2: "DESIGN", 3: "BUILD" };

function rows(codes: string[] = CODES): Rows {
  const each = <T,>(value: T) => codes.map(() => value);
  return {
    schema: "totalfloat.rows/1",
    file: "x.xer",
    count: codes.length,
    id: codes.map((_, k) => k + 1),
    project: each(1),
    code: codes,
    name: codes.map((c) => `Name ${c}`),
    type: each("task"),
    status: each("not-started"),
    critical: each(false),
    calendar: each("Five day"),
    wbs_path: each("P"),
    original_duration_hours: each(8),
    remaining_duration_hours: each(8),
    total_float_hours: codes.map((_, k) => [0, 16, 40, 80][k] ?? 100),
    free_float_hours: each(0),
    early_start: each("2026-01-05 08:00"),
    early_finish: each("2026-01-05 17:00"),
    late_start: each(null),
    late_finish: each(null),
    actual_start: each(null),
    actual_finish: each(null),
    constraint: each(null),
    constraint_date: each(null),
    secondary_constraint: each(null),
    secondary_constraint_date: each(null),
  } as unknown as Rows;
}

/** A server that knows the Phase code and float, as `query` would answer. */
function finder(asked: QueryParams[]): GridFinder {
  return {
    groups: [{ field: "code:Phase", title: "Phase", kind: "code", order: new Map([["DESIGN", 0], ["BUILD", 1]]) }],
    codes: [{ type: "Phase", values: ["DESIGN", "BUILD"] }],
    query: (params) => {
      asked.push(params);
      if (params.code === "Phase=NOPE" || String(params.any ?? "").includes("Phase=NOPE")) {
        return Promise.reject(new ApiError(400, "no-such-code", "no activity code matches Phase=NOPE", "use TYPE=VALUE"));
      }
      if (String(params.any ?? "").includes("Phase=BUILDX")) {
        return Promise.reject(new ApiError(400, "no-such-code", "no activity code matches Phase=BUILDX", "use TYPE=VALUE"));
      }
      const max = params.float_max === undefined ? Infinity : Number(params.float_max);
      const floats = [0, 16, 40, 80];
      const found = [1, 2, 3, 4].filter(
        (id) => (floats[id - 1] ?? 0) <= max && (params.code === undefined || params.code === `Phase=${PHASE[id]}`),
      );
      const report = {
        schema: "totalfloat.query/1",
        rows: found.map((id) => ({ id, "code:Phase": PHASE[id] ?? null })),
      };
      return Promise.resolve(report as unknown as QueryReport);
    },
  };
}

/** The codes and band titles the screen-reader mirror lists, top to bottom. */
function listed(container: HTMLElement): string[] {
  return [...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => {
    const cell = tr.querySelector("td");
    return tr.getAttribute("aria-expanded") === null ? (cell?.textContent ?? "") : `[${cell?.textContent?.split(",")[0]}]`;
  });
}

async function settle() {
  await act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
}

describe("the grid's built filters and grouping", () => {
  it("applies a built filter as the server's query finds it", async () => {
    const asked: QueryParams[] = [];
    const { container } = render(<Grid rows={rows()} finder={finder(asked)} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "float_max" } });
    fireEvent.change(screen.getByLabelText("Total float at most (h) value"), { target: { value: "20" } });
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "code" } });
    fireEvent.change(screen.getByLabelText("Activity code value"), { target: { value: "BUILD" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    expect(asked.at(-1)).toEqual({ float_max: "20", code: "Phase=BUILD", columns: "id" });
    expect(listed(container)).toEqual(["[WBS P]", "A1"]);
    expect(chips()).toHaveLength(2);
    expect(container.querySelector(".grid-count")?.textContent).toBe("1 of 4");

    // Clearing shows everything again.
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    await settle();
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2", "A3", "A4"]);
  });

  it("says why the server refused a filter, and keeps the one applied", async () => {
    const asked: QueryParams[] = [];
    const { container } = render(<Grid rows={rows()} finder={finder(asked)} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "code" } });
    // A value the list doesn't hold, typed.
    fireEvent.change(screen.getByLabelText("Activity code value"), { target: { value: "NOPE" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    expect(screen.getByRole("alert").textContent).toBe("no activity code matches Phase=NOPE. Use TYPE=VALUE");
    expect(listed(container)).toHaveLength(5);
    expect(chips()).toEqual([]);
  });

  it("asks for a value before applying", async () => {
    render(<Grid rows={rows()} finder={finder([])} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "wbs" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    expect(screen.getByRole("alert").textContent).toBe("Give each condition a value, or remove it.");
  });

  it("groups by an activity code, in the code's own order, and back to the WBS", async () => {
    const asked: QueryParams[] = [];
    const { container } = render(<Grid rows={rows()} finder={finder(asked)} />);
    fireEvent.change(grouping("Group by"), { target: { value: "code:Phase" } });
    await settle();
    expect(asked.at(-1)).toEqual({ columns: "id,code:Phase" });
    expect(listed(container)).toEqual(["[Phase: DESIGN]", "A2", "[Phase: BUILD]", "A1", "A3", "[Phase: (no value)]", "A4"]);
    fireEvent.change(grouping("Group by"), { target: { value: "none" } });
    expect(listed(container)).toEqual(["A1", "A2", "A3", "A4"]);
    fireEvent.change(grouping("Group by"), { target: { value: "wbs" } });
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2", "A3", "A4"]);
  });

  it("groups by the WBS, then the code, then status, and drops a level", async () => {
    const asked: QueryParams[] = [];
    const reported: GridLayout[] = [];
    const { container } = render(<Grid rows={rows()} finder={finder(asked)} onLayout={(l) => reported.push(l)} />);
    fireEvent.change(grouping("Then group by"), { target: { value: "code:Phase" } });
    await settle();
    expect(asked.at(-1)).toEqual({ columns: "id,code:Phase" });
    expect(listed(container)).toEqual([
      "[WBS P]",
      "[Phase: DESIGN]",
      "A2",
      "[Phase: BUILD]",
      "A1",
      "A3",
      "[Phase: (no value)]",
      "A4",
    ]);
    expect(reported.at(-1)).toMatchObject({ grouped: true, groupBy: null, thenBy: ["code:Phase"] });
    // The levels before aren't offered again.
    const third = grouping("Then group by") as HTMLSelectElement;
    const offered = [...third.options].map((o) => o.value);
    expect(offered).not.toContain("wbs");
    expect(offered).not.toContain("code:Phase");
    fireEvent.change(third, { target: { value: "status" } });
    // Nor are levels after it: level 2's menu doesn't offer status.
    const second = grouping("Then by, level 2") as HTMLSelectElement;
    expect([...second.options].map((o) => o.value)).not.toContain("status");
    expect(listed(container).slice(0, 4)).toEqual(["[WBS P]", "[Phase: DESIGN]", "[Status: Not started]", "A2"]);
    // Taking the code away leaves the WBS, then status.
    fireEvent.change(grouping("Then by, level 2"), { target: { value: "" } });
    expect(listed(container)).toEqual(["[WBS P]", "[Status: Not started]", "A1", "A2", "A3", "A4"]);
    expect(reported.at(-1)).toMatchObject({ thenBy: ["status"] });
    // No grouping, no further levels.
    fireEvent.change(grouping("Group by"), { target: { value: "none" } });
    expect(grouping("Group by") && screen.queryByLabelText("Then group by")).toBeNull();
    expect(reported.at(-1)).not.toHaveProperty("thenBy");
  });

  it("keeps one level's problem when another level's values come", async () => {
    const asked: QueryParams[] = [];
    const base = finder(asked);
    const found: GridFinder = {
      ...base,
      groups: [...base.groups, { field: "code:Area", title: "Area", kind: "code" }],
      query: (params) =>
        String(params.columns).includes("Area")
          ? Promise.reject(new ApiError(500, "serve-failed", "the server stopped", ""))
          : base.query(params),
    };
    const layout: GridLayout = {
      columns: [{ field: "code", width: 90 }],
      sort: [],
      grouped: true,
      groupBy: "code:Area",
      thenBy: ["code:Phase"],
      text: "",
      quick: [],
      built: [],
    };
    const { container } = render(<Grid rows={rows()} finder={found} layout={layout} />);
    await settle();
    expect(screen.getAllByRole("alert").map((a) => a.textContent)).toEqual([
      "The rows couldn't be grouped by Area: the server stopped",
    ]);
    // Area's values never came, so Phase waits behind it: WBS bands only.
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2", "A3", "A4"]);
  });

  it("finds them again when the rows change", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    const { rerender } = render(<Grid rows={rows()} finder={found} />);
    fireEvent.change(grouping("Group by"), { target: { value: "code:Phase" } });
    await settle();
    const before = asked.length;
    rerender(<Grid rows={rows()} finder={found} />);
    await settle();
    expect(asked.length).toBe(before + 1);
  });

  it("drops an apply still waiting when the filter is cleared", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    const answers: (() => void)[] = [];
    const slow: GridFinder = {
      ...found,
      query: (params) => new Promise((done) => answers.push(() => done(found.query(params)))),
    };
    const { container } = render(<Grid rows={rows()} finder={slow} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "float_max" } });
    fireEvent.change(screen.getByLabelText("Total float at most (h) value"), { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    for (const answer of answers) answer();
    await settle();
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2", "A3", "A4"]);
    expect(chips()).toEqual([]);
  });

  it("removes two chips one after another, though the first hasn't been answered, and closes the builder", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    const answers: (() => void)[] = [];
    let slowly = false;
    const slow: GridFinder = {
      ...found,
      query: (params) => (slowly ? new Promise((done) => answers.push(() => done(found.query(params)))) : found.query(params)),
    };
    render(<Grid rows={rows()} finder={slow} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "float_max" } });
    fireEvent.change(screen.getByLabelText("Total float at most (h) value"), { target: { value: "20" } });
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "critical" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    expect(chips()).toEqual(["Float ≤ 20h×", "Critical×"]);
    slowly = true;
    fireEvent.click(screen.getByRole("button", { name: "Remove the filter Float ≤ 20h" }));
    // Removing one closes the builder, which still held it.
    expect(document.querySelector(".filter-builder")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Remove the filter Critical" }));
    for (const answer of answers) answer();
    await settle();
    expect(chips()).toEqual([]);
  });

  it("finds a filter again when the rows change, and says when it can't", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    let failing = false;
    const flaky: GridFinder = {
      ...found,
      query: (params) => (failing ? Promise.reject(new Error("the server went away")) : found.query(params)),
    };
    const { container, rerender } = render(<Grid rows={rows()} finder={flaky} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "float_max" } });
    fireEvent.change(screen.getByLabelText("Total float at most (h) value"), { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    // Applying asks once, not again for the same rows.
    expect(asked).toEqual([{ float_max: "20", columns: "id" }]);
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2"]);
    failing = true;
    rerender(<Grid rows={rows()} finder={flaky} />);
    await settle();
    expect(screen.getByRole("alert").textContent).toMatch(/couldn't be found again.*the server went away/);
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2"]);
    failing = false;
    rerender(<Grid rows={rows()} finder={flaky} />);
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("stops asking for a field's values once the rows aren't grouped", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    const { rerender } = render(<Grid rows={rows()} finder={found} />);
    fireEvent.change(grouping("Group by"), { target: { value: "code:Phase" } });
    await settle();
    fireEvent.change(grouping("Group by"), { target: { value: "none" } });
    const before = asked.length;
    rerender(<Grid rows={rows()} finder={found} />);
    await settle();
    expect(asked.length).toBe(before);
  });

  it("escapes a comma in the field grouped by", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    const commas: GridFinder = { ...found, groups: [{ field: "udf:Cost, phase 2", title: "Cost, phase 2", kind: "udf" }] };
    render(<Grid rows={rows()} finder={commas} />);
    fireEvent.change(grouping("Group by"), { target: { value: "udf:Cost, phase 2" } });
    await settle();
    expect(asked.at(-1)).toEqual({ columns: "id,udf:Cost\\, phase 2" });
  });

  it("drops the filter when no conditions are applied", async () => {
    const asked: QueryParams[] = [];
    const { container, rerender } = render(<Grid rows={rows()} finder={finder(asked)} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "float_max" } });
    fireEvent.change(screen.getByLabelText("Total float at most (h) value"), { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Remove Total float at most (h)" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    // An activity added since shows too: no filter is left on the ids.
    rerender(<Grid rows={rows([...CODES, "A5"])} finder={finder(asked)} />);
    await settle();
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2", "A3", "A4", "A5"]);
    expect(chips()).toEqual([]);
  });

  it("keeps an apply's answer over a refetch of the conditions it replaced", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    let slow = false;
    const waiting: (() => void)[] = [];
    const gated: GridFinder = {
      ...found,
      query: (params) =>
        slow ? new Promise((done) => waiting.push(() => done(found.query(params)))) : found.query(params),
    };
    const { container, rerender } = render(<Grid rows={rows()} finder={gated} />);
    openBuilder();
    fireEvent.change(screen.getByLabelText("Add a condition"), { target: { value: "float_max" } });
    const value = screen.getByLabelText("Total float at most (h) value");
    fireEvent.change(value, { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await settle();
    expect(listed(container)).toEqual(["[WBS P]", "A1"]);
    // Apply float ≤ 20 slowly; the rows change while it waits, refetching float ≤ 0.
    slow = true;
    fireEvent.change(value, { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    rerender(<Grid rows={rows()} finder={gated} />);
    await settle();
    // The apply answers first, then the refetch of what it replaced; then any asked since.
    while (waiting.length > 0) {
      waiting.shift()?.();
      await settle();
    }
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2"]);
    expect(asked.at(-1)).toEqual({ float_max: "20", columns: "id" });
  });
});

describe("a layout's grouping and built filter", () => {
  const LAYOUT = {
    columns: [{ field: "code" as const, width: 90 }],
    sort: [],
    grouped: true,
    groupBy: "code:Phase",
    text: "",
    quick: [],
    built: [
      { field: "code" as const, value: "Phase=BUILD" },
      { field: "float_max" as const, value: "20" },
    ],
  };

  it("reopens grouped by its code with its filter, and says it's set out so", async () => {
    const asked: QueryParams[] = [];
    const reported: unknown[] = [];
    const { container } = render(
      <Grid rows={rows()} finder={finder(asked)} layout={LAYOUT} onLayout={(l) => reported.push(l)} />,
    );
    await settle();
    expect(asked).toContainEqual({ code: "Phase=BUILD", float_max: "20", columns: "id" });
    expect(listed(container)).toEqual(["[Phase: BUILD]", "A1"]);
    expect((grouping("Group by") as HTMLSelectElement).value).toBe("code:Phase");
    expect(chips()).toHaveLength(2);
    expect(reported.at(-1)).toEqual(LAYOUT);
  });

  it("takes a chip removed after another layout opens from that layout's filter", async () => {
    const asked: QueryParams[] = [];
    const found = finder(asked);
    const { rerender } = render(<Grid rows={rows()} finder={found} layout={LAYOUT} />);
    await settle();
    expect(chips()).toEqual(["Phase=BUILD×", "Float ≤ 20h×"]);
    fireEvent.click(screen.getByRole("button", { name: "Remove the filter Float ≤ 20h" }));
    await settle();
    expect(chips()).toEqual(["Phase=BUILD×"]);
    const other = { ...LAYOUT, built: [{ field: "critical" as const, value: "" }, { field: "float_max" as const, value: "40" }] };
    rerender(<Grid rows={rows()} finder={found} layout={other} />);
    await settle();
    expect(chips()).toEqual(["Critical×", "Float ≤ 40h×"]);
    fireEvent.click(screen.getByRole("button", { name: "Remove the filter Critical" }));
    await settle();
    expect(chips()).toEqual(["Float ≤ 40h×"]);
  });

  it("groups by the WBS and drops a code the schedule doesn't have, saying so", async () => {
    const asked: QueryParams[] = [];
    const missing = {
      ...LAYOUT,
      groupBy: "udf:Gone",
      built: [
        { field: "code" as const, value: "Phase=NOPE" },
        { field: "float_max" as const, value: "20" },
      ],
    };
    const { container } = render(<Grid rows={rows()} finder={finder(asked)} layout={missing} />);
    await settle();
    expect(asked.at(-1)).toEqual({ float_max: "20", columns: "id" });
    expect(listed(container)).toEqual(["[WBS P]", "A1", "A2"]);
    const said = screen.getAllByRole("alert").map((a) => a.textContent);
    expect(said).toEqual([
      "This schedule has no activity code Phase=NOPE, so the layout's filter goes without it.",
      "This schedule has no udf:Gone, so rows are grouped by the WBS.",
    ]);
    expect(chips()).toHaveLength(1);
  });

  it("drops the “any of” group that needs a code the schedule doesn't have, and keeps the rest", async () => {
    const asked: QueryParams[] = [];
    const missing = {
      ...LAYOUT,
      built: [
        { field: "code" as const, value: "Phase=BUILD" },
        { field: "code" as const, value: "Phase=NOPE", group: 0 },
        { field: "float_max" as const, value: "20", group: 1 },
      ],
    };
    render(<Grid rows={rows()} finder={finder(asked)} layout={missing} />);
    await settle();
    expect(asked.at(-1)).toEqual({ code: "Phase=BUILD", any: JSON.stringify([{ float_max: "20" }]), columns: "id" });
    expect(screen.getAllByRole("alert").map((a) => a.textContent)).toEqual([
      'This schedule has no activity code Phase=NOPE, so the layout\'s filter goes without the "any of" group that needs it.',
    ]);
  });

  it("drops the code the server names, not one whose value it starts with", async () => {
    const asked: QueryParams[] = [];
    const missing = {
      ...LAYOUT,
      built: [
        { field: "code" as const, value: "Phase=BUILD" },
        { field: "code" as const, value: "Phase=BUILDX", group: 0 },
        { field: "float_max" as const, value: "20", group: 1 },
      ],
    };
    render(<Grid rows={rows()} finder={finder(asked)} layout={missing} />);
    await settle();
    expect(asked.at(-1)).toEqual({ code: "Phase=BUILD", any: JSON.stringify([{ float_max: "20" }]), columns: "id" });
  });

  it("forgets a layout's filter still waiting when the filter is cleared", async () => {
    const answers: (() => void)[] = [];
    const found = finder([]);
    const slow: GridFinder = {
      ...found,
      query: (params) => new Promise((done) => answers.push(() => done(found.query(params)))),
    };
    const reported: GridLayout[] = [];
    const { container, rerender } = render(
      <Grid rows={rows()} finder={slow} layout={LAYOUT} onLayout={(l) => reported.push(l)} />,
    );
    await settle();
    openBuilder();
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    await settle();
    expect(reported.at(-1)?.built).toEqual([]);
    // The layout's answer comes late, and a new finder doesn't bring it back.
    for (const answer of answers.splice(0)) answer();
    await settle();
    rerender(<Grid rows={rows()} finder={{ ...slow }} layout={LAYOUT} onLayout={(l) => reported.push(l)} />);
    await settle();
    for (const answer of answers.splice(0)) answer();
    await settle();
    expect(reported.at(-1)?.built).toEqual([]);
    expect(chips()).toEqual([]);
    expect(container.querySelector(".grid-count")?.textContent).toBe("4 of 4");
  });
});
