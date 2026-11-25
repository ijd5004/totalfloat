// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { Links, Rows } from "../api";
import { View } from "../view";
import { PROJECT_LIMIT } from "./project";
import { ProjectNetwork } from "./ProjectNetwork";

afterEach(cleanup);

// jsdom has no ResizeObserver.
beforeAll(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterAll(() => {
  vi.unstubAllGlobals();
});

function rows(n: number): Rows {
  const ids = Array.from({ length: n }, (_, k) => k + 1);
  const each = <T,>(value: T) => ids.map(() => value);
  return {
    schema: "totalfloat.rows/1",
    file: "x.xer",
    count: n,
    id: ids,
    project: each(1),
    code: ids.map((id) => `A${id}`),
    name: ids.map((id) => `Activity ${id}`),
    early_start: each("2026-01-05 08:00"),
    early_finish: each("2026-01-05 17:00"),
    total_float_hours: each(0),
    critical: each(false),
  } as unknown as Rows;
}

// A1 → A2 → A3, A1 → A3.
const LINKS = {
  schema: "totalfloat.links/1",
  count: 3,
  id: [11, 12, 13],
  predecessor: [1, 2, 1],
  successor: [2, 3, 3],
  type: ["FS", "FS", "FS"],
  lag_hours: [0, 0, 0],
  driving: [true, true, false],
} as unknown as Links;

/** A view of `n` activities, with the grid showing `lines` of them. */
function viewOf(n: number, lines: number[] = Array.from({ length: n }, (_, k) => k), active = 0): View {
  const view = new View();
  view.publish({
    rows: rows(n),
    arranged: { lines: Int32Array.from(lines), bands: [], matched: lines.length },
    top: 0,
    height: 400,
    rowHeight: 22,
    active,
    bodyTop: 0,
  });
  return view;
}

const boxes = () => screen.queryAllByRole("button", { name: /select it in the grid/ }).map((b) => b.getAttribute("aria-label")?.split(" ")[0]);

describe("the whole-project network", () => {
  it("draws every activity the grid shows, in columns by logic", () => {
    const { container } = render(<ProjectNetwork view={viewOf(3)} links={LINKS} onNeighbourhood={() => {}} />);
    expect(boxes().sort()).toEqual(["A1", "A2", "A3"]);
    expect(screen.getByText("3 activities in 3 columns by logic")).toBeTruthy();
    expect(container.querySelectorAll(".network-link")).toHaveLength(3);
    expect(container.querySelectorAll(".network-link.driving")).toHaveLength(2);
  });

  it("follows the grid's filter and selection, and selects in the grid when a box is clicked", () => {
    const view = viewOf(3, [0, 2], 0);
    const reveal = vi.spyOn(view, "reveal");
    const { container } = render(<ProjectNetwork view={view} links={LINKS} onNeighbourhood={() => {}} />);
    expect(boxes().sort()).toEqual(["A1", "A3"]);
    expect(container.querySelector(".network-box.centre")?.getAttribute("aria-label")).toMatch(/^A1 /);
    act(() => {
      const now = view.current();
      if (now) view.publish({ ...now, active: 1 });
    });
    expect(container.querySelector(".network-box.centre")?.getAttribute("aria-label")).toMatch(/^A3 /);
    fireEvent.click(screen.getByRole("button", { name: /^A1 / }));
    expect(reveal).toHaveBeenCalledWith(1);
  });

  it("says when there are too many to draw, and offers the network around the selection", () => {
    const onNeighbourhood = vi.fn();
    render(<ProjectNetwork view={viewOf(PROJECT_LIMIT + 1)} links={LINKS} onNeighbourhood={onNeighbourhood} />);
    expect(boxes()).toEqual([]);
    expect(screen.getByRole("status").textContent).toContain(`${(PROJECT_LIMIT + 1).toLocaleString()} activities`);
    fireEvent.click(screen.getByText("Network around the selection"));
    expect(onNeighbourhood).toHaveBeenCalled();
  });

  it("scrolls to a new selection, and leaves the scroll alone when the network is rebuilt", () => {
    const view = viewOf(3, [0, 1, 2], 2);
    const { container } = render(<ProjectNetwork view={view} links={LINKS} onNeighbourhood={() => {}} />);
    const scroller = container.querySelector(".project-network-scroll") as HTMLDivElement;
    const first = scroller.scrollLeft;
    expect(first).toBeGreaterThan(0);
    scroller.scrollLeft = 5000;
    // New rows, the same selection: the reader's place is kept.
    act(() => {
      const now = view.current();
      if (now) view.publish({ ...now, rows: { ...now.rows }, arranged: { ...now.arranged } });
    });
    expect(scroller.scrollLeft).toBe(5000);
    act(() => {
      const now = view.current();
      if (now) view.publish({ ...now, active: 0 });
    });
    expect(scroller.scrollLeft).not.toBe(5000);
    // A band, then the same activity again: it's shown again.
    const back = scroller.scrollLeft;
    scroller.scrollLeft = 7000;
    act(() => {
      const now = view.current();
      if (now) view.publish({ ...now, arranged: { ...now.arranged, lines: Int32Array.from([-1, 0, 1, 2]) }, active: 0 });
    });
    act(() => {
      const now = view.current();
      if (now) view.publish({ ...now, active: 1 });
    });
    expect(scroller.scrollLeft).toBe(back);
  });

  it("waits for the links", () => {
    render(<ProjectNetwork view={viewOf(3)} links={null} onNeighbourhood={() => {}} />);
    expect(screen.getByText(/still on their way/)).toBeTruthy();
  });
});
