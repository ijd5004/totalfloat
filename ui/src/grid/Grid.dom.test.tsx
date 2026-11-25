// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { Rows } from "../api";
import { comparedKey } from "./columns";
import { type CellKey, type CellProblem, cellKey } from "../edit/cells";
import { Grid, type GridEditing } from "./Grid";

afterEach(cleanup);

// jsdom lays nothing out; give the grid's body a screen's height, so the
// rows on screen are mirrored as in a browser.
const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
});
afterAll(() => {
  if (height) Object.defineProperty(HTMLElement.prototype, "clientHeight", height);
});

function rows(codes: string[], ids: number[]): Rows {
  const each = <T,>(value: T) => codes.map(() => value);
  return {
    schema: "totalfloat.rows/1",
    file: "x.xer",
    count: codes.length,
    id: ids,
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
    total_float_hours: each(0),
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

/** The text of the first cell in the active cell's mirrored row. */
function activeCode(container: HTMLElement): string | undefined {
  const body = container.querySelector(".grid-body");
  const id = body?.getAttribute("aria-activedescendant") ?? "";
  return container.querySelector(`[id="${id}"]`)?.closest("tr")?.querySelector("td")?.textContent ?? undefined;
}

describe("the grid's selection", () => {
  it("stays on its activity when fresh rows arrive without one above it", () => {
    const { container, rerender } = render(<Grid rows={rows(["A100", "A101", "A102"], [100, 101, 102])} />);
    const body = container.querySelector(".grid-body") as HTMLElement;
    fireEvent.focus(body);
    // Line 0 is the WBS band; A101 is line 2.
    fireEvent.keyDown(body, { key: "ArrowDown" });
    fireEvent.keyDown(body, { key: "ArrowDown" });
    expect(activeCode(container)).toBe("A101");
    rerender(<Grid rows={rows(["A101", "A102"], [101, 102])} />);
    expect(activeCode(container)).toBe("A101");
  });

  it("marks every cell of a range selected, for screen readers", () => {
    const { container } = render(<Grid rows={rows(["A1", "A2", "A3"], [1, 2, 3])} />);
    const body = container.querySelector(".grid-body") as HTMLElement;
    fireEvent.keyDown(body, { key: "ArrowDown" });
    fireEvent.keyDown(body, { key: "ArrowDown", shiftKey: true });
    fireEvent.keyDown(body, { key: "ArrowRight", shiftKey: true });
    const selected = container.querySelectorAll('#grid-mirror td[aria-selected="true"]');
    expect(selected.length).toBe(4);
    expect(container.querySelector("#grid-mirror")?.getAttribute("aria-multiselectable")).toBe("true");
  });

  it("shows the Before and After columns only while comparing", () => {
    const shown = (container: HTMLElement) => [...container.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const { container, rerender } = render(<Grid rows={rows(["A1"], [1])} />);
    expect(shown(container)).not.toContain("Before");
    rerender(<Grid rows={rows(["A1"], [1])} compared={{ projects: new Map(), activities: new Map() }} />);
    expect(shown(container).slice(0, 4)).toEqual(["Activity ID", "Activity name", "Before", "After"]);
  });

  it("keeps the selection on its column as Before and After come and go", () => {
    const compared = { projects: new Map(), activities: new Map() };
    const { container, rerender } = render(<Grid rows={rows(["A1"], [1])} />);
    const body = container.querySelector(".grid-body") as HTMLElement;
    // Past the WBS band, to A1's Original.
    fireEvent.keyDown(body, { key: "ArrowDown" });
    fireEvent.keyDown(body, { key: "ArrowRight" });
    fireEvent.keyDown(body, { key: "ArrowRight" });
    const activeTitle = () => {
      const titles = [...container.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
      const cell = container.querySelector('#grid-mirror td[aria-selected="true"]') as HTMLTableCellElement | null;
      return cell ? titles[cell.cellIndex] : null;
    };
    expect(activeTitle()).toBe("Original (d)");
    rerender(<Grid rows={rows(["A1"], [1])} compared={compared} />);
    expect(activeTitle()).toBe("Original (d)");
    rerender(<Grid rows={rows(["A1"], [1])} />);
    expect(activeTitle()).toBe("Original (d)");
  });

  it("re-sorts by Before or After for a new comparison, and drops the sort when they hide", () => {
    const make = (a1: string, a2: string) => ({
      projects: new Map([[1, "P"]]),
      activities: new Map([
        [comparedKey("P", "A1"), { added: false, fields: [{ field: "calendar", from: "x", to: a1 }] }],
        [comparedKey("P", "A2"), { added: false, fields: [{ field: "calendar", from: "x", to: a2 }] }],
      ]),
    });
    const codes = (container: HTMLElement) =>
      [...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => (tr as HTMLTableRowElement).cells[0]?.textContent).filter((c) => c?.startsWith("A"));
    // The same rows throughout, as when only the comparison changes.
    const two = rows(["A1", "A2"], [1, 2]);
    const { container, rerender } = render(<Grid rows={two} compared={make("b", "a")} />);
    fireEvent.click([...container.querySelectorAll(".grid-head-button")].find((b) => b.textContent?.includes("After")) as Element);
    expect(codes(container)).toEqual(["A2", "A1"]);
    rerender(<Grid rows={two} compared={make("a", "b")} />);
    expect(codes(container)).toEqual(["A1", "A2"]);
    rerender(<Grid rows={two} compared={make("b", "a")} />);
    expect(codes(container)).toEqual(["A2", "A1"]);
    // Hidden, they sort nothing: back to the default order.
    rerender(<Grid rows={two} />);
    expect(codes(container)).toEqual(["A1", "A2"]);
    // Shown again, they come back unsorted: the sort went with them.
    rerender(<Grid rows={two} compared={make("b", "a")} />);
    expect(codes(container)).toEqual(["A1", "A2"]);
  });

  it("sets itself out by a layout given, and says how it's set out", () => {
    const reported: unknown[] = [];
    const layout = {
      columns: [
        { field: "name" as const, width: 300 },
        { field: "code" as const, width: 90 },
      ],
      sort: [{ column: "code" as const, descending: true }],
      grouped: false,
      groupBy: null,
      text: "",
      quick: [],
      built: [],
    };
    const { container } = render(<Grid rows={rows(["A1", "A2"], [1, 2])} layout={layout} onLayout={(l) => reported.push(l)} />);
    const titles = [...container.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    expect(titles).toEqual(["Activity name", "Activity ID"]);
    const codes = [...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => (tr as HTMLTableRowElement).cells[1]?.textContent);
    expect(codes).toEqual(["A2", "A1"]);
    expect(reported.at(-1)).toEqual(layout);
  });

  it("moves the screen-reader mirror once scrolling settles, not every frame", () => {
    vi.useFakeTimers();
    try {
      const many = rows(
        Array.from({ length: 400 }, (_, k) => `A${k + 1}`),
        Array.from({ length: 400 }, (_, k) => k + 1),
      );
      const { container } = render(<Grid rows={many} />);
      const body = container.querySelector(".grid-body") as HTMLElement;
      // The last row mirrored: the first is the active row, which stays.
      const lastMirrored = () => [...container.querySelectorAll("#grid-mirror tbody tr")].at(-1)?.getAttribute("aria-rowindex");
      const before = lastMirrored();
      for (const top of [500, 1500, 3000]) {
        body.scrollTop = top;
        fireEvent.scroll(body);
        act(() => vi.advanceTimersByTime(30));
      }
      // Still scrolling: the mirror hasn't been rebuilt.
      expect(lastMirrored()).toBe(before);
      act(() => vi.advanceTimersByTime(500));
      expect(Number(lastMirrored())).toBeGreaterThan(Number(before));
    } finally {
      vi.useRealTimers();
    }
  });
  it("shows the refusal on screen while scrolling, before the mirror catches up", () => {
    const many = rows(
      Array.from({ length: 400 }, (_, k) => `A${k + 1}`),
      Array.from({ length: 400 }, (_, k) => k + 1),
    );
    const editing: GridEditing = {
      enabled: true,
      values: new Map<CellKey, string>(),
      marks: new Map<CellKey, CellProblem>([[cellKey(300, "original"), { message: "too long", hint: "shorter" }]]),
      previewed: false,
      commit: () => {},
      apply: () => {},
      discard: () => {},
      say: () => {},
      isShortcut: () => false,
    };
    const { container } = render(<Grid rows={many} editing={editing} />);
    expect(container.querySelector(".cell-tip")).toBeNull();
    const body = container.querySelector(".grid-body") as HTMLElement;
    // A300 is on line 300, under the band: scroll it on screen, and don't wait.
    body.scrollTop = 300 * 22 - 100;
    fireEvent.scroll(body);
    const tip = container.querySelector(".cell-tip") as HTMLElement;
    expect(tip?.textContent).toBe("too longshorter");
    expect(tip.style.top).toBe(`${301 * 22 + 2}px`);
  });
});

describe("the column titles", () => {
  // jsdom measures no text: 7 px a character, wider than the 13 px bold
  // system font measured about 6.7 in Chromium, plus the header's 12 px of
  // padding. `e2e/grid.mjs` checks the same in a browser.
  const fits = (title: string, width: number) => title.length * 7 + 12 <= width;

  it("fit their default widths, with the full name kept for tooltips and screen readers", () => {
    const { container } = render(<Grid rows={rows(["A1"], [1])} />);
    const heads = [...container.querySelectorAll<HTMLElement>(".grid-head")].map((head) => ({
      shows: head.querySelector(".grid-head-title")?.textContent ?? "",
      named: head.querySelector(".grid-head-button")?.getAttribute("aria-label") ?? "",
      tip: head.querySelector(".grid-head-button")?.getAttribute("title") ?? "",
      width: parseFloat(head.style.width),
    }));
    expect(heads.length).toBeGreaterThan(10);
    expect(heads.filter((h) => !fits(h.shows, h.width))).toEqual([]);
    const original = heads.find((h) => h.named === "Original (d)");
    expect(original?.shows).toBe("Orig (d)");
    expect(original?.tip.startsWith("Original (d).")).toBe(true);
  });
});

describe("the filter chips", () => {
  it("shows each filter applied as a chip that removes it, with the statuses together", () => {
    render(<Grid rows={rows(["A1", "A2"], [1, 2])} />);
    const chips = () => [...document.querySelectorAll('[aria-label="Filters applied"] .chip')].map((c) => c.textContent);
    expect(chips()).toEqual([]);
    const add = (name: string) => {
      if (!document.querySelector('[role="menu"]')) fireEvent.click(document.querySelector(".tool-button") as HTMLElement);
      const item = [...document.querySelectorAll('[role="menuitemcheckbox"]')].find((i) => i.textContent === name) as HTMLElement;
      fireEvent.click(item);
    };
    add("Critical");
    add("Not started");
    add("In progress");
    expect(chips()).toEqual(["Critical×", "Not started or in progress×"]);
    expect(document.querySelector(".chip .chip-swatch")).not.toBeNull();
    // The menu marks what's on.
    expect([...document.querySelectorAll('[role="menuitemcheckbox"]')].map((i) => i.getAttribute("aria-checked"))).toEqual(["true", "true", "true", "false"]);
    fireEvent.click(document.querySelector('[aria-label="Remove the filter Critical"]') as HTMLElement);
    expect(chips()).toEqual(["Not started or in progress×"]);
    fireEvent.click(document.querySelector('[aria-label="Remove the filter Not started or in progress"]') as HTMLElement);
    expect(chips()).toEqual([]);
  });
});

describe("counts said in words", () => {
  const band = (container: HTMLElement) => container.querySelector("#grid-mirror tbody tr[aria-expanded] td")?.textContent;
  const status = (container: HTMLElement) => container.querySelector(".status-count")?.textContent;

  it("says one activity in the singular", () => {
    const { container } = render(<Grid rows={rows(["A1"], [1])} />);
    expect(band(container)).toBe("WBS P, 1 activity");
    expect(status(container)).toBe("1 of 1 activity");
  });

  it("and more in the plural", () => {
    const { container } = render(<Grid rows={rows(["A1", "A2"], [1, 2])} />);
    expect(band(container)).toBe("WBS P, 2 activities");
    expect(status(container)).toBe("2 of 2 activities");
  });
});
