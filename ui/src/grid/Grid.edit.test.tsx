// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { type CellKey, type CellProblem, type Typed, cellKey } from "../edit/cells";
import { sample } from "../edit/fixture";
import { View } from "../view";
import { Grid, type GridEditing, parseRange, pastedCells, rangeText } from "./Grid";

afterEach(cleanup);

const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
});
afterAll(() => {
  if (height) Object.defineProperty(HTMLElement.prototype, "clientHeight", height);
});

function editing(change: Partial<GridEditing> = {}) {
  const commits: Typed[][] = [];
  const said: string[] = [];
  const value: GridEditing = {
    enabled: true,
    values: new Map<CellKey, string>(),
    marks: new Map<CellKey, CellProblem>(),
    previewed: false,
    commit: (typed) => void commits.push(typed),
    apply: vi.fn(),
    discard: vi.fn(),
    say: (text) => void said.push(text),
    isShortcut: (event) => event.key === "?" || event.key === "/",
    ...change,
  };
  return { value, commits, said };
}

/** The grid on the sample rows, with the active cell on A1 (line 1, under the band) in `column`. */
function grid(edit: GridEditing, column: number) {
  const rendered = render(<Grid rows={sample()} editing={edit} />);
  const body = rendered.container.querySelector(".grid-body") as HTMLElement;
  body.focus();
  fireEvent.keyDown(body, { key: "ArrowDown" });
  for (let c = 0; c < column; c++) fireEvent.keyDown(body, { key: "ArrowRight" });
  return { ...rendered, body };
}

// Columns: 0 code, 1 name, 2 original, 3 remaining, 4 start, ...
describe("editing in the grid", () => {
  it("opens an input on typing, and commits it with Enter, moving down", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 2);
    fireEvent.keyDown(body, { key: "5" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    expect(input.value).toBe("5");
    fireEvent.change(input, { target: { value: "5d" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(commits).toEqual([[{ id: 1, column: "original", text: "5d" }]]);
    expect(container.querySelector(".cell-input")).toBeNull();
    expect(body.getAttribute("aria-activedescendant")).toBe("grid-cell-2-2");
  });

  it("opens with the value on F2 and Enter, and Escape leaves it as it was", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 2);
    fireEvent.keyDown(body, { key: "F2" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    expect(input.value).toBe("40h");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(container.querySelector(".cell-input")).toBeNull();
    fireEvent.keyDown(body, { key: "Enter" });
    expect((container.querySelector(".cell-input") as HTMLInputElement).value).toBe("40h");
    expect(commits).toEqual([]);
  });

  it("sends nothing when the input closes as it opened, so it can't undo a change made meanwhile", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 1);
    fireEvent.keyDown(body, { key: "F2" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    fireEvent.keyDown(input, { key: "Tab" });
    expect(commits).toEqual([]);
    expect(body.getAttribute("aria-activedescendant")).toBe("grid-cell-1-2");
  });

  it("sends an emptied cell that typing opened, as clearing it", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 1);
    fireEvent.keyDown(body, { key: "x" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.keyDown(input, { key: "Tab" });
    expect(commits).toEqual([[{ id: 1, column: "name", text: "" }]]);
  });

  it("commits with Tab, moving right", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 1);
    fireEvent.keyDown(body, { key: "F2" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Pour slab" } });
    fireEvent.keyDown(input, { key: "Tab" });
    expect(commits).toEqual([[{ id: 1, column: "name", text: "Pour slab" }]]);
    expect(body.getAttribute("aria-activedescendant")).toBe("grid-cell-1-2");
  });

  it("records a waiting preview on Enter rather than opening the cell", () => {
    const { value } = editing({ previewed: true });
    const { container, body } = grid(value, 2);
    fireEvent.keyDown(body, { key: "Enter" });
    expect(value.apply).toHaveBeenCalledOnce();
    expect(container.querySelector(".cell-input")).toBeNull();
  });

  it("types the letters and signs that were once shortcuts into the cell", () => {
    for (const key of ["e", "d", "l", "+", "-", "="]) {
      const { value } = editing();
      const { container, body, unmount } = grid(value, 1);
      fireEvent.keyDown(body, { key });
      expect((container.querySelector(".cell-input") as HTMLInputElement | null)?.value).toBe(key);
      unmount();
    }
  });

  it("leaves shortcut keys to the app, and says when a column can't be edited", () => {
    const { value, said } = editing();
    const { container, body } = grid(value, 0);
    fireEvent.keyDown(body, { key: "?" });
    expect(container.querySelector(".cell-input")).toBeNull();
    fireEvent.keyDown(body, { key: "x" });
    expect(container.querySelector(".cell-input")).toBeNull();
    expect(said).toEqual(["Activity ID can't be changed here."]);
  });

  it("pastes a range from the active cell as one commit", () => {
    const { value, commits } = editing();
    const { body } = grid(value, 2);
    fireEvent.paste(body, { clipboardData: { getData: () => "5d\t3d\n6d\t2d\n" } });
    expect(commits).toEqual([
      [
        { id: 1, column: "original", text: "5d" },
        { id: 1, column: "remaining", text: "3d" },
        { id: 2, column: "original", text: "6d" },
        { id: 2, column: "remaining", text: "2d" },
      ],
    ]);
  });

  it("leaves Enter that ends an input method's composition to it", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 1);
    fireEvent.keyDown(body, { key: "F2" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "にほ" } });
    fireEvent.keyDown(input, { key: "Enter", isComposing: true, keyCode: 229 });
    expect(commits).toEqual([]);
    expect(container.querySelector(".cell-input")).not.toBeNull();
  });

  it("copies typed values as they show", () => {
    const rows = sample();
    const typed = new Map([[cellKey(1, "original"), "80h"]]);
    const lines = Int32Array.from([-1, 0, 1]);
    expect(rangeText(rows, lines, ["code", "original"], { line: 1, column: 0 }, { line: 2, column: 1 }, typed)).toBe(
      // Rows without hours in a day show hours, marked as such.
      "A1\t80h\nA2\t40h",
    );
  });

  it("copies dates as a cell takes them, whatever format they show in", () => {
    const rows = sample();
    const lines = Int32Array.from([-1, 0]);
    expect(rangeText(rows, lines, ["code", "start"], { line: 1, column: 0 }, { line: 1, column: 1 })).toBe("A1\t2026-01-05 08:00");
  });

  it("takes copy and paste sent to the page while the grid has the focus", () => {
    const { value, commits } = editing();
    grid(value, 2);
    // Firefox sends them to <body> when the page has no selection.
    fireEvent.paste(document.body, { clipboardData: { getData: () => "5d" } });
    expect(commits).toEqual([[{ id: 1, column: "original", text: "5d" }]]);
    const set = vi.fn();
    fireEvent.copy(document.body, { clipboardData: { setData: set } });
    expect(set).toHaveBeenCalledWith("text/plain", "40h");
  });

  it("leaves copy and paste in a cell's input, or anywhere else, to the browser", () => {
    const { value, commits } = editing();
    const { container, body } = grid(value, 2);
    fireEvent.keyDown(body, { key: "F2" });
    const input = container.querySelector(".cell-input") as HTMLInputElement;
    input.focus();
    expect(fireEvent.paste(input, { clipboardData: { getData: () => "5d" } })).toBe(true);
    input.blur();
    const field = document.body.appendChild(document.createElement("input"));
    field.focus();
    expect(fireEvent.paste(field, { clipboardData: { getData: () => "6d" } })).toBe(true);
    expect(fireEvent.copy(field, { clipboardData: { setData: vi.fn() } })).toBe(true);
    field.remove();
    expect(commits).toEqual([]);
  });

  it("ignores a paste with no text", () => {
    const { value, commits } = editing();
    const { body } = grid(value, 2);
    fireEvent.paste(body, { clipboardData: { getData: () => "" } });
    expect(commits).toEqual([]);
  });

  it("names the column and activity being edited", () => {
    const { value } = editing();
    const { container, body } = grid(value, 2);
    fireEvent.keyDown(body, { key: "F2" });
    expect(container.querySelector(".cell-input")?.getAttribute("aria-label")).toBe("Original (d) of A1");
  });

  it("shows typed values and refusals in the mirror", () => {
    const key = cellKey(1, "original");
    const { value } = editing({
      values: new Map([[key, "5x"]]),
      marks: new Map([[key, { message: `"5x" isn't a duration`, hint: "type hours or days, such as 40, 40h or 5d" }]]),
    });
    const { container } = grid(value, 2);
    const cell = container.querySelector("#grid-cell-1-2") as HTMLElement;
    expect(cell.textContent).toBe("5x");
    expect(cell.getAttribute("aria-invalid")).toBe("true");
    expect(container.querySelector(".cell-tip")?.textContent).toContain(`"5x" isn't a duration`);
  });

  it("shows a refusal's tooltip at once, though the cursor has moved on", () => {
    const { value } = editing({
      marks: new Map([[cellKey(2, "remaining"), { message: "too long", hint: "shorter" }]]),
    });
    // The active cell is A1's code; the refused cell is A2's remaining.
    const { container } = grid(value, 0);
    const tip = container.querySelector(".cell-tip") as HTMLElement;
    expect(tip.textContent).toBe("too longshorter");
    // Under line 2 (A2), in the fourth column.
    expect(tip.style.top).toBe(`${3 * 22 + 2}px`);
  });
});

describe("pasted ranges", () => {
  it("reads tab-separated lines, as spreadsheets copy them", () => {
    expect(parseRange("a\tb\r\nc\td\r\n")).toEqual([
      ["a", "b"],
      ["c", "d"],
    ]);
    expect(parseRange("one")).toEqual([["one"]]);
  });

  it("passes over bands, and counts cells past the grid's edge", () => {
    const rows = sample();
    // A band, A1, a band, A2, A3.
    const lines = Int32Array.from([-1, 0, -2, 1, 2]);
    const columns = ["name", "original"] as const;
    expect(pastedCells(rows, lines, [...columns], { line: 1, column: 0 }, [["x"], ["y"]]).typed).toEqual([
      { id: 1, column: "name", text: "x" },
      { id: 2, column: "name", text: "y" },
    ]);
    expect(pastedCells(rows, lines, [...columns], { line: 3, column: 1 }, [["1", "2"], ["3"], ["4"]]).outside).toBe(2);
  });
});

describe("the grid and the Gantt", () => {
  it("moves the active cell to an activity the Gantt asks for", () => {
    const view = new View();
    const { container } = render(<Grid rows={sample()} view={view} />);
    act(() => view.reveal(3));
    // A3 is line 3, under the band.
    expect(container.querySelector(".grid-body")?.getAttribute("aria-activedescendant")).toBe("grid-cell-3-0");
  });

  it("shows an activity a filter or a collapsed band hides before selecting it", () => {
    const view = new View();
    const { container } = render(<Grid rows={sample()} view={view} />);
    const filter = container.querySelector(".grid-filter") as HTMLInputElement;
    fireEvent.change(filter, { target: { value: "A1" } });
    expect(container.querySelector(".grid-count")?.textContent).toBe("1 of 3");
    act(() => view.reveal(3));
    expect(filter.value).toBe("");
    expect(container.querySelector(".grid-count")?.textContent).toBe("3 of 3");
    expect(container.querySelector(".grid-body")?.getAttribute("aria-activedescendant")).toBe("grid-cell-3-0");
    // Collapse the band, then reveal A2 under it.
    const body = container.querySelector(".grid-body") as HTMLElement;
    fireEvent.keyDown(body, { key: "Home", ctrlKey: true });
    fireEvent.keyDown(body, { key: "ArrowLeft" });
    expect(container.querySelectorAll("#grid-mirror tbody tr")).toHaveLength(1);
    act(() => view.reveal(2));
    expect(container.querySelector(".grid-body")?.getAttribute("aria-activedescendant")).toBe("grid-cell-2-0");
  });

  it("filters to a driving path while asked, and shows everything when it's cleared", () => {
    const { container, rerender } = render(<Grid rows={sample()} path={new Set([1, 3])} />);
    const count = () => container.querySelector(".grid-count")?.textContent;
    expect(count()).toBe("3 of 3");
    fireEvent.click(screen.getByRole("button", { name: "Driving path" }));
    expect(count()).toBe("2 of 3");
    rerender(<Grid rows={sample()} path={null} />);
    expect(count()).toBe("3 of 3");
    expect(screen.queryByRole("button", { name: "Driving path" })).toBeNull();
    // The next path starts unfiltered.
    rerender(<Grid rows={sample()} path={new Set([2])} />);
    expect(count()).toBe("3 of 3");
    expect(screen.getByRole("button", { name: "Driving path" }).getAttribute("aria-pressed")).toBe("false");
  });
});
