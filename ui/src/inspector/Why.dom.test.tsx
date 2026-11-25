// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Api } from "../api";
import { sample } from "../edit/fixture";
import type { DrivingPath } from "../gantt/draw";
import { DEFAULT_ARRANGEMENT, arrange, buildModel } from "../grid/model";
import { View } from "../view";
import { Inspector, type InspectorEditing } from "./Inspector";
import { ASK_DELAY_MS } from "./Why";

afterEach(cleanup);

/**
 * A server explaining each activity in its own words, so a wrong answer
 * shows: "A2 is set by A1", "A3 is set by A2". With `deferred`, answers wait
 * until the test releases them, in any order.
 */
function server(deferred = false) {
  const asked: string[] = [];
  const waiting: { key: string; release: () => void }[] = [];
  const ids: Record<string, number> = { A1: 1, A2: 2, A3: 3 };
  const api = new Api("token", (input) => {
    const url = new URL(String(input), "http://x");
    // The inspector asks for the activity's details too; those aren't explanations.
    if (!url.pathname.endsWith("/explain")) {
      return Promise.resolve(new Response(JSON.stringify({ schema: "totalfloat.query/1", file: "x", total: 0, offset: 0, limit: 0, returned: 0, columns: [], rows: [] }), { status: 200 }));
    }
    const code = url.searchParams.get("activity") ?? "";
    const field = url.searchParams.get("field") ?? "";
    asked.push(`${code} ${field}`);
    const before = code === "A3" ? "A2" : "A1";
    const chain = (on: string, by: string, relationship: number) => ({
      field: on,
      date: "2026-01-06 08:00",
      origin: "data-date",
      total_steps: 2,
      steps: [
        { activity: ids[code], code, field: on, hours_per_day: 8, date: "2026-01-06 08:00", text: `${code} ${on} is set by ${by}`, relationship },
        { activity: ids[by], code: by, field: on, hours_per_day: 8, date: "2026-01-05 08:00", text: `${by} starts at the data date` },
      ],
    });
    // Total float compares two dates: a chain for each, the second through A3.
    const body =
      field === "total_float"
        ? { schema: "totalfloat.explain/1", total_float_hours: 0, chains: [chain("early_start", before, 9), chain("late_start", "A3", 11)] }
        : { schema: "totalfloat.explain/1", chains: [chain(field, before, 9)] };
    const answer = () => new Response(JSON.stringify(body), { status: 200 });
    if (!deferred) return Promise.resolve(answer());
    return new Promise((done) => waiting.push({ key: `${code} ${field}`, release: () => done(answer()) }));
  });
  const release = async (key: string) => {
    const at = waiting.findIndex((w) => w.key === key);
    await act(async () => {
      waiting.splice(at, 1)[0]?.release();
      await Promise.resolve();
    });
  };
  return { api, asked, release };
}

/** Lets the selection rest long enough to be asked about. */
async function rest() {
  await act(async () => {
    await new Promise((done) => setTimeout(done, ASK_DELAY_MS + 30));
  });
}

function viewOn(line: number) {
  const view = new View();
  const rows = sample();
  const arranged = arrange(buildModel(rows), DEFAULT_ARRANGEMENT);
  const shown = { rows, arranged, top: 0, height: 400, rowHeight: 22, active: line, bodyTop: 0 };
  view.publish(shown);
  return { view, shown };
}

const READ_ONLY: InspectorEditing = { enabled: false, values: new Map(), marks: new Map(), commit: () => {} };

/** The inspector, whose "Why" section is the explain panel that was. */
function ExplainPanel(props: { api: Api; view: View; onClose: () => void; onChain?: (chain: DrivingPath | null) => void }) {
  return <Inspector links={null} fields={null} edit={READ_ONLY} {...props} />;
}

describe("the inspector's Why section", () => {
  it("explains the selected activity, numbering each activity along the chain", async () => {
    const { api, asked } = server();
    // Line 2 is A2, under the band.
    const { view } = viewOn(2);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    expect(await screen.findByText("A2 early_start is set by A1")).toBeTruthy();
    expect(asked).toEqual(["A2 early_start"]);
    expect(screen.getByRole("region", { name: "Activity A2" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Why it starts" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Why does A2 start on 06 Jan 26?" })).toBeTruthy();
    const stops = [...document.querySelectorAll(".explain-stop")].map((b) => b.textContent?.slice(0, 3));
    expect(stops).toEqual(["1A2", "2A1"]);
  });

  it("selects a stop's activity when it's clicked", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    const revealed: number[] = [];
    view.revealWith((id) => revealed.push(id));
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    fireEvent.click((await screen.findByText("A1 starts at the data date")).closest("button") as HTMLElement);
    expect(revealed).toEqual([1]);
  });

  it("explains another date from its tab, by click or arrow key", async () => {
    const { api, asked } = server();
    const { view } = viewOn(2);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    await screen.findByText(/A2 early_start/);
    fireEvent.click(screen.getByRole("tab", { name: "Float" }));
    expect(await screen.findByText(/A2 late_start is set by A3/)).toBeTruthy();
    // Arrows move between tabs, wrapping, and only the chosen tab is a tab stop.
    fireEvent.keyDown(screen.getByRole("tab", { name: "Float" }), { key: "ArrowRight" });
    expect(await screen.findByText(/A2 early_start/)).toBeTruthy();
    const start = screen.getByRole("tab", { name: "Start" });
    expect(start.getAttribute("aria-selected")).toBe("true");
    expect(start.tabIndex).toBe(0);
    expect(screen.getByRole("tab", { name: "Finish" }).tabIndex).toBe(-1);
    expect(screen.getByRole("tabpanel").getAttribute("aria-labelledby")).toBe(start.id);
    expect(asked).toEqual(["A2 early_start", "A2 total_float", "A2 early_start"]);
  });

  it("shows only the latest selection's answer, whatever order answers come in", async () => {
    const { api, release } = server(true);
    const { view, shown } = viewOn(2);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    await rest();
    // A3 is selected while A2's answer is still on its way; A2's arrives last.
    act(() => view.publish({ ...shown, active: 3 }));
    await rest();
    await release("A3 early_start");
    await release("A2 early_start");
    expect(screen.getByText("A3 early_start is set by A2")).toBeTruthy();
    expect(screen.queryByText(/A2 early_start is set by/)).toBeNull();
  });

  it("asks once the selection rests, for the row it rests on", async () => {
    const { api, asked } = server();
    const { view, shown } = viewOn(1);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    act(() => view.publish({ ...shown, active: 2 }));
    act(() => view.publish({ ...shown, active: 3 }));
    expect(await screen.findByText(/A3 early_start/)).toBeTruthy();
    expect(asked).toEqual(["A3 early_start"]);
  });

  it("asks again for fresh rows, but not for a scroll", async () => {
    const { api, asked } = server();
    const { view, shown } = viewOn(3);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    await screen.findByText(/A3 early_start/);
    act(() => view.publish({ ...shown, top: 22 }));
    const rows = sample();
    act(() => view.publish({ ...shown, rows, arranged: arrange(buildModel(rows), DEFAULT_ARRANGEMENT) }));
    await screen.findByText(/A3 early_start/);
    expect(asked).toEqual(["A3 early_start", "A3 early_start"]);
  });

  it("clears when nothing is selected, as when a filter hides everything", async () => {
    const { api } = server();
    const { view, shown } = viewOn(2);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    await screen.findByText(/A2 early_start/);
    act(() => view.publish({ ...shown, arranged: { lines: new Int32Array(0), bands: [], matched: 0 }, active: 0 }));
    expect(screen.queryByText(/A2 early_start/)).toBeNull();
    expect(screen.getByText(/^Select an activity in the grid or the Gantt/)).toBeTruthy();
  });

  it("says what zero float means without claiming more than it knows", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    render(<ExplainPanel api={api} view={view} onClose={() => {}} />);
    await screen.findByText(/A2 early_start/);
    fireEvent.click(screen.getByRole("tab", { name: "Float" }));
    expect(await screen.findByText("It has no float: if it slips at all, it passes its latest dates.")).toBeTruthy();
  });

  it("maps the chain pointed into on the Gantt, when there are several", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    const chains: (DrivingPath | null)[] = [];
    render(<ExplainPanel api={api} view={view} onClose={() => {}} onChain={(c) => chains.push(c)} />);
    await screen.findByText(/A2 early_start/);
    fireEvent.click(screen.getByRole("tab", { name: "Float" }));
    const third = await screen.findByText("A3 starts at the data date");
    // The first chain, A2 back to A1, until the second is pointed into. The
    // Gantt hears it from an effect, which can land just after the text.
    await waitFor(() => expect([...(chains.at(-1)?.ids ?? [])].sort()).toEqual([1, 2]));
    fireEvent.mouseEnter(third.closest("button") as HTMLElement);
    expect([...(chains.at(-1)?.ids ?? [])].sort()).toEqual([2, 3]);
    expect(chains.at(-1)?.focus).toBe(3);
    expect([...(chains.at(-1)?.relationships ?? [])]).toEqual([11]);
  });

  it("hands the Gantt its chain, numbered as the panel numbers it, and what it points at", async () => {
    const { api } = server();
    const { view } = viewOn(2);
    const chains: (DrivingPath | null)[] = [];
    render(<ExplainPanel api={api} view={view} onClose={() => {}} onChain={(c) => chains.push(c)} />);
    const second = await screen.findByText("A1 starts at the data date");
    // The Gantt hears it from an effect, which can land just after the text.
    await waitFor(() =>
      expect([...(chains.at(-1)?.numbers ?? [])]).toEqual([
        [2, 1],
        [1, 2],
      ]),
    );
    expect([...(chains.at(-1)?.relationships ?? [])]).toEqual([9]);
    // Pointing at a stop points at its bar.
    fireEvent.mouseEnter(second.closest("button") as HTMLElement);
    expect(chains.at(-1)?.focus).toBe(1);
  });
});
