// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Api, type Fields, type Links } from "../api";
import { DEFAULT_DATES, showDates } from "../dates";
import { cellKey } from "../edit/cells";
import { Editing } from "../edit/editing";
import { sample } from "../edit/fixture";
import { DEFAULT_ARRANGEMENT, arrange, buildModel } from "../grid/model";
import { View } from "../view";
import { Inspector, type InspectorEditing } from "./Inspector";

afterEach(cleanup);

const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });

/** A1 → A2 (FS, driving) and A2 → A3 (SS +1 day); A1 → A3 (FF). */
const LINKS: Links = {
  schema: "totalfloat.links/1",
  count: 3,
  id: [11, 12, 13],
  predecessor: [1, 2, 1],
  successor: [2, 3, 3],
  type: ["FS", "SS", "FF"],
  lag_hours: [0, 8, 0],
  driving: [true, false, true],
};

const FIELDS: Fields = {
  schema: "totalfloat.fields/1",
  activity_codes: [
    { name: "Phase", values: [{ value: "DESIGN", description: "" }] },
    { name: "Area, east", values: [{ value: "E1", description: "" }] },
  ],
  activity_udfs: [
    { label: "Owner", data_type: "text" },
    { label: "Unused", data_type: "text" },
  ],
};

/** A server answering explain with one step, and query with A2's details; keeps the queries asked. */
function server() {
  const queries: URLSearchParams[] = [];
  const edits: Record<string, unknown>[] = [];
  const api = new Api("token", (input, init) => {
    const url = new URL(String(input), "http://x");
    if (url.pathname.endsWith("/explain")) {
      const code = url.searchParams.get("activity") ?? "";
      return Promise.resolve(
        json({
          schema: "totalfloat.explain/1",
          chains: [{ field: "early_start", date: "2026-01-05 08:00", origin: "data-date", total_steps: 1, steps: [{ activity: 2, code, field: "early_start", hours_per_day: 8, date: "2026-01-05 08:00", text: `${code} starts at the data date` }] }],
        }),
      );
    }
    if (url.pathname.endsWith("/query")) {
      queries.push(url.searchParams);
      return Promise.resolve(
        json({
          schema: "totalfloat.query/1",
          file: "x",
          total: 2,
          offset: 0,
          limit: 0,
          returned: 2,
          columns: [],
          rows: [
            // Another activity the text matches, which isn't the one asked about.
            { id: 3, resources: "WRONG", roles: "", notes: null },
            {
              id: 2,
              resources: "CREW-1, CRANE",
              roles: "PM",
              notes: "Scope: First line\nsecond line\n\nRisk: Late steel",
              "code:Phase": "DESIGN",
              "code:Area, east": null,
              "udf:Owner": "Pat",
              "udf:Pour": "2026-01-05 08:00",
              "udf:Unused": null,
            },
          ],
        }),
      );
    }
    if (url.pathname.endsWith("/edit")) {
      edits.push(JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>);
      return Promise.resolve(
        json({
          schema: "totalfloat.edit/1",
          file: "plant.tfdb@main",
          output: null,
          applied: ["A2: original duration 40h -> 6d"],
          warnings: [],
          projects: [{ id: 7, code: "P", scheduled_finish: "2026-01-12 17:00", previous_finish: "2026-01-09 17:00", changed_dates_or_float: 1, became_critical_total: 0, left_critical_total: 0 }],
        }),
      );
    }
    throw new Error(`nothing to answer ${url.pathname}`);
  });
  return { api, queries, edits };
}

function viewOn(line: number, rows = sample()) {
  const view = new View();
  const arranged = arrange(buildModel(rows), DEFAULT_ARRANGEMENT);
  const shown = { rows, arranged, top: 0, height: 400, rowHeight: 22, active: line, bodyTop: 0 };
  view.publish(shown);
  return { view, shown, rows };
}

const READ_ONLY: InspectorEditing = { enabled: false, values: new Map(), marks: new Map(), commit: () => {} };

/** Lets the details' delay pass and their answer land. */
async function settle(ms = 200) {
  await act(async () => {
    await new Promise((done) => setTimeout(done, ms));
  });
}

describe("the inspector", () => {
  it("heads with the activity's ID, name, status and a Critical marker", async () => {
    const { api } = server();
    // Line 2 is A2, under the band.
    const { view } = viewOn(2, sample({ critical: [false, true, false], status: ["not-started", "in-progress", "not-started"] }));
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    const inspector = screen.getByRole("region", { name: "Activity A2" });
    expect(within(inspector).getByRole("heading", { level: 2 }).textContent).toBe("Name A2");
    expect(inspector.querySelector(".inspector-tags")?.textContent).toBe("In progressCritical");
    await settle();
  });

  it("shows dates, durations, float, calendar and constraint in two columns", async () => {
    const { api } = server();
    const { view } = viewOn(2, sample({ total_float_hours: [0, 16, 0], hours_per_day: [8, 8, 8] }));
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    const dates = screen.getByRole("region", { name: "Dates and duration" });
    const pairs = [...dates.querySelectorAll(".inspector-field")].map((f) => `${f.querySelector("dt")?.textContent}=${f.querySelector("dd")?.textContent}`);
    expect(pairs).toEqual([
      "Original duration=5d",
      "Remaining=5d",
      "Start=05 Jan 26",
      "Finish=09 Jan 26",
      "Actual start=None",
      "Actual finish=None",
      "Total float=2d",
      "Free float=0d",
      "Calendar=Five day",
      "Constraint=None",
      "Constraint date=None",
    ]);
    await settle();
  });

  it("lists predecessors and successors from the links, with type, lag and driving, and selects one when clicked", async () => {
    const { api } = server();
    const { view } = viewOn(2, sample({ hours_per_day: [8, 8, 8] }));
    const revealed: number[] = [];
    view.revealWith((id) => revealed.push(id));
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    const before = screen.getByRole("region", { name: "Predecessors 1" });
    const after = screen.getByRole("region", { name: "Successors 1" });
    expect(within(before).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["A1 Name A1, FS, driving"]);
    expect(within(after).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["A3 Name A3, SS +1d"]);
    expect(before.querySelector(".inspector-dot.driving")).not.toBeNull();
    expect(after.querySelector(".inspector-dot.driving")).toBeNull();
    fireEvent.click(within(after).getByRole("button"));
    expect(revealed).toEqual([3]);
    await settle();
  });

  it("says when nothing comes before or after", async () => {
    const { api } = server();
    const { view } = viewOn(1);
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    expect(screen.getByText("None: nothing comes before it, so its start is open.")).toBeTruthy();
    // Both of A1's successors are listed, driving first, then as the server lists them.
    const after = screen.getByRole("region", { name: "Successors 2" });
    expect([...after.querySelectorAll(".inspector-link-code")].map((c) => c.textContent)).toEqual(["A2", "A3"]);
    await settle();
  });

  it("shows resources, codes and fields, and notes, collapsed with counts, from the activity's own row", async () => {
    const { api, queries } = server();
    const { view } = viewOn(2);
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    await settle();
    expect(queries).toHaveLength(1);
    expect(queries[0]?.get("text")).toBe("A2");
    // Across the file: code types are the whole file's, and a project without one refuses its column.
    expect(queries[0]?.has("project")).toBe(false);
    // A comma in a code type's name is escaped in the list of columns.
    expect(queries[0]?.get("columns")).toBe("id,resources,roles,notes,code:Phase,code:Area\\, east,udf:Owner,udf:Unused");
    const summaries = [...document.querySelectorAll(".inspector-more summary")].map((s) => s.textContent);
    expect(summaries).toEqual(["Resources3", "Codes and fields2", "Notes2"]);
    const sections = [...document.querySelectorAll<HTMLDetailsElement>(".inspector-more")];
    expect(sections.every((d) => !d.open)).toBe(true);
    fireEvent.click(sections[0]?.querySelector("summary") as HTMLElement);
    expect(sections[0]?.open).toBe(true);
    expect([...(sections[0]?.querySelectorAll("li") ?? [])].map((li) => li.textContent)).toEqual(["CREW-1", "CRANE", "PM (role)"]);
    expect([...(sections[1]?.querySelectorAll("dl > div") ?? [])].map((d) => d.textContent)).toEqual(["PhaseDESIGN", "OwnerPat"]);
    expect([...(sections[2]?.querySelectorAll("dl > div") ?? [])].map((d) => d.textContent)).toEqual(["ScopeFirst line\nsecond line", "RiskLate steel"]);
  });

  it("says when the details didn't find the activity, rather than that it has none", async () => {
    const { api } = server();
    // A1's query answers only A3 and A2.
    const { view } = viewOn(1);
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    await settle();
    expect(screen.getAllByText(/^A1 wasn't found when its details were asked for/)).toHaveLength(3);
    expect(screen.queryByText("No notes.")).toBeNull();
  });

  it("shows a date field in the date format chosen, and again when it changes", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    const fields: Fields = { ...FIELDS, activity_udfs: [{ label: "Pour", data_type: "start-date" }] };
    const shown = () => [...document.querySelectorAll(".inspector-pairs dl > div, .inspector-pairs > div")].map((d) => d.textContent);
    try {
      const { rerender } = render(<Inspector api={api} view={view} links={LINKS} fields={fields} edit={READ_ONLY} onClose={() => {}} />);
      await settle();
      expect(shown()).toContain("Pour05 Jan 26");
      const format = { style: "dd/mm/yyyy", time: false } as const;
      showDates(format);
      rerender(<Inspector api={api} view={view} links={LINKS} fields={fields} edit={READ_ONLY} dates={format} onClose={() => {}} />);
      expect(shown()).toContain("Pour05/01/2026");
    } finally {
      showDates(DEFAULT_DATES);
    }
  });

  it("asks for details once the selection rests, not for each row stepped through", async () => {
    const { api, queries } = server();
    const { view, shown } = viewOn(1);
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    act(() => view.publish({ ...shown, active: 2 }));
    act(() => view.publish({ ...shown, active: 3 }));
    // A scroll alone asks nothing.
    act(() => view.publish({ ...shown, active: 3, top: 22 }));
    await settle();
    expect(queries.map((q) => q.get("text"))).toEqual(["A3"]);
    expect(screen.getByRole("region", { name: "Activity A3" })).toBeTruthy();
  });

  it("says an activity added and waiting is added on apply, and asks the server nothing about it", async () => {
    const { api, queries } = server();
    // Line 3 is A10, added and waiting: the grid gives it a negative id.
    const { view } = viewOn(3, sample({ id: [1, 2, -1], code: ["A1", "A2", "A10"], name: ["Name A1", "Name A2", "Survey"], original_duration_hours: [40, 40, 16], hours_per_day: [8, 8, 8] }));
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    await settle();
    const inspector = screen.getByRole("region", { name: "Activity A10" });
    expect(within(inspector).getByRole("heading", { level: 2 }).textContent).toBe("Survey");
    expect(inspector.querySelector(".inspector-tags")?.textContent).toBe("Waiting to be added");
    expect(inspector.textContent).toContain("Original duration2d");
    expect(inspector.textContent).toContain("It's added when you apply the edits waiting.");
    expect(queries).toHaveLength(0);
    expect(screen.queryByText("Why it starts")).toBeNull();
  });

  it("shows read-only values as text when the schedule can't be edited", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={READ_ONLY} onClose={() => {}} />);
    expect(screen.queryByRole("button", { name: /Original duration/ })).toBeNull();
    await settle();
  });

  it("edits through the grid's flow: the same pending edit and preview as typing it in the grid", async () => {
    // Typed in the inspector.
    const fromInspector = server();
    const editing = new Editing(fromInspector.api, null);
    const { view, rows } = viewOn(2, sample({ hours_per_day: [8, 8, 8] }));
    editing.show({ rows, head: "h1", writable: true });
    const edit = (): InspectorEditing => {
      const state = editing.state();
      return { enabled: true, values: state.values, marks: state.marks, commit: (typed) => void editing.commit(typed) };
    };
    const { rerender } = render(<Inspector api={fromInspector.api} view={view} links={LINKS} fields={FIELDS} edit={edit()} onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Original duration: 5d. Change it" }));
    const input = screen.getByRole("textbox", { name: "Original duration of A2" }) as HTMLInputElement;
    expect(input.value).toBe("5d");
    fireEvent.change(input, { target: { value: "6d" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await settle(0);
    rerender(<Inspector api={fromInspector.api} view={view} links={LINKS} fields={FIELDS} edit={edit()} onClose={() => {}} />);
    // The value waits, marked, as the grid's cell would show it.
    const waiting = screen.getByRole("button", { name: "Original duration: 6d. Change it" });
    expect(waiting.classList.contains("typed")).toBe(true);
    expect(document.activeElement).toBe(waiting);
    expect(editing.state().pending?.preview?.projects[0]?.scheduled_finish).toBe("2026-01-12 17:00");
    expect(editing.state().values.get(cellKey(2, "original"))).toBe("6d");

    // Typed in the grid: the grid's input commits its cell the same way.
    const fromGrid = server();
    const grid = new Editing(fromGrid.api, null);
    grid.show({ rows, head: "h1", writable: true });
    await grid.commit([{ id: 2, column: "original", text: "6d" }]);
    expect(fromInspector.edits).toEqual(fromGrid.edits);
    expect(fromInspector.edits[0]).toMatchObject({ dry_run: true, edits: [{ op: "set_duration", activity: "A2", original: "6d" }] });
    expect(editing.state().pending?.batch).toEqual(grid.state().pending?.batch);
    await settle();
  });

  it("puts a value back on Escape, and sends nothing for one left as it was", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    const committed: unknown[] = [];
    const edit: InspectorEditing = { enabled: true, values: new Map(), marks: new Map(), commit: (typed) => committed.push(typed) };
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={edit} onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Calendar: Five day. Change it" }));
    const input = screen.getByRole("combobox", { name: "Calendar of A2" });
    fireEvent.change(input, { target: { value: "Seven day" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(committed).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Calendar: Five day. Change it" }));
    fireEvent.blur(screen.getByRole("combobox", { name: "Calendar of A2" }));
    expect(committed).toEqual([]);
    // Enter commits once, though the input blurs as it goes.
    fireEvent.click(screen.getByRole("button", { name: "Activity name: Name A2. Change it" }));
    const name = screen.getByRole("textbox", { name: "Activity name of A2" });
    fireEvent.change(name, { target: { value: "Pour slab" } });
    fireEvent.keyDown(name, { key: "Enter" });
    fireEvent.blur(name);
    expect(committed).toEqual([[{ id: 2, column: "name", text: "Pour slab" }]]);
    await settle();
  });

  it("marks a refused value with the server's reason", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    const edit: InspectorEditing = {
      enabled: true,
      values: new Map([[cellKey(2, "constraint_date"), "soon"]]),
      marks: new Map([[cellKey(2, "constraint_date"), { message: `"soon" isn't a date and time`, hint: "type it as 2026-03-02 08:00" }]]),
      commit: () => {},
    };
    render(<Inspector api={api} view={view} links={LINKS} fields={FIELDS} edit={edit} onClose={() => {}} />);
    const value = screen.getByRole("button", { name: "Constraint date: soon. Change it" });
    expect(value.classList.contains("refused")).toBe(true);
    expect(document.getElementById(value.getAttribute("aria-describedby") ?? "")?.textContent).toBe(`"soon" isn't a date and time (type it as 2026-03-02 08:00)`);
    await settle();
  });
});
