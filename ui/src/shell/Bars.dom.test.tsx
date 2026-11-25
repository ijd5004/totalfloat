// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { EditState, Editing } from "../edit/editing";
import type { Connection } from "../events";
import { EditBar, TitleBlock } from "./Bars";

afterEach(cleanup);

function state(change: Partial<EditState> = {}): EditState {
  return {
    pending: null,
    values: new Map(),
    marks: new Map(),
    notice: null,
    banner: null,
    busy: false,
    previewing: true,
    message: "",
    made: [],
    queued: [],
    dropping: new Set(),
    ...change,
  } as EditState;
}

describe("the status bar's edits", () => {
  it("with edits pending, counts them, says how the finish moves, and offers Review and Apply", () => {
    const apply = vi.fn();
    const opened = vi.fn();
    const editing = { apply, setPreviewing: vi.fn() } as unknown as Editing;
    const waiting = state({
      pending: { typed: [], batch: { edits: [], cells: [] }, head: "h1", preview: { projects: [] } } as unknown as EditState["pending"],
      values: new Map([
        ["1:original", "5d"],
        ["2:original", "6d"],
      ]) as unknown as EditState["values"],
    });
    render(<EditBar editing={editing} state={waiting} onOpen={opened} moved="+4d" calculated={new Date(2026, 0, 5, 14, 5)} saved="Saved on main" />);
    const group = screen.getByRole("group", { name: "Edits pending" });
    expect(group.textContent).toContain("2 edits pending");
    expect(group.textContent).toContain("Finish +4d");
    // While edits wait, the calculation time gives way to them.
    expect(screen.queryByText(/Calculated at/)).toBeNull();
    fireEvent.click(within(group).getByRole("button", { name: "Review" }));
    fireEvent.click(within(group).getByRole("button", { name: "Apply" }));
    expect(opened).toHaveBeenCalledOnce();
    expect(apply).toHaveBeenCalledOnce();
  });

  it("with none, says when the schedule was calculated and where it's saved", () => {
    const editing = { apply: vi.fn(), setPreviewing: vi.fn() } as unknown as Editing;
    render(<EditBar editing={editing} state={state()} onOpen={() => {}} calculated={new Date(2026, 0, 5, 14, 5)} saved="Saved on main" />);
    expect(screen.queryByRole("group", { name: "Edits pending" })).toBeNull();
    expect(screen.getByText("Calculated at 14:05")).toBeTruthy();
    expect(screen.getByText("Saved on main")).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: "Preview" })).toBeTruthy();
  });
});

describe("the title block's connection state", () => {
  // Below 1,360 px only the dot shows; its tooltip says the state in the same
  // words as its label, which a screen reader reads.
  it.each<[Connection, string]>([
    ["connecting", "Connecting…"],
    ["live", "Live"],
    ["disconnected", "Disconnected"],
    ["signed-out", "Signed out: open the link `serve` printed"],
  ])("when %s, has a tooltip saying %s", (connection, words) => {
    render(
      <TitleBlock
        summary={null}
        connection={connection}
        mac={false}
        onSearch={() => {}}
        project={null}
        views={[{ id: "schedule", label: "Schedule" }]}
        view="schedule"
        onView={() => {}}
        outlook={{ float: null }}
        menu={null}
      />,
    );
    const state = screen.getByRole("status");
    expect(state.getAttribute("title")).toBe(words);
    expect(state.textContent).toBe(words);
  });
});
