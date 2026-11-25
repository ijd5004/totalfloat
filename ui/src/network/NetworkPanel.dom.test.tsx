// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Links, Rows } from "../api";
import { View } from "../view";
import { NetworkPanel } from "./NetworkPanel";

afterEach(cleanup);

const ROWS = {
  schema: "totalfloat.rows/1",
  file: "x.xer",
  count: 4,
  id: [1, 2, 3, 4],
  project: [1, 1, 1, 1],
  code: ["A1", "A2", "A3", "A4"],
  name: ["Excavate", "Pour", "Cure", "Strip"],
  early_start: ["2026-01-05 08:00", "2026-01-06 08:00", "2026-01-08 08:00", "2026-01-09 08:00"],
  early_finish: ["2026-01-05 17:00", "2026-01-07 17:00", "2026-01-08 17:00", "2026-01-09 17:00"],
  total_float_hours: [0, 0, 8, 16],
  critical: [true, true, false, false],
} as unknown as Rows;

// A1 → A2 (driving) → A3 → A4.
const LINKS = {
  schema: "totalfloat.links/1",
  count: 3,
  id: [11, 12, 13],
  predecessor: [1, 2, 3],
  successor: [2, 3, 4],
  type: ["FS", "FS", "SS"],
  lag_hours: [0, 0, 8],
  driving: [true, false, false],
} as unknown as Links;

/** A view with the grid's selection on activity `line`. */
function viewOn(line: number): View {
  const view = new View();
  view.publish({
    rows: ROWS,
    arranged: { lines: Int32Array.from([0, 1, 2, 3]), bands: [], matched: 4 },
    top: 0,
    height: 400,
    rowHeight: 22,
    active: line,
    bodyTop: 0,
  });
  return view;
}

const boxes = () => screen.getAllByRole("button", { name: /select it in the grid/ }).map((b) => b.getAttribute("aria-label")?.split(" ")[0]);

describe("the network panel", () => {
  it("shows the selected activity's neighbours, and follows the selection", () => {
    const view = viewOn(1);
    const { container } = render(<NetworkPanel view={view} links={LINKS} onClose={() => {}} />);
    expect(screen.getByRole("heading").textContent).toBe("Network: A2");
    expect(boxes().sort()).toEqual(["A1", "A2", "A3", "A4"]);
    expect(screen.getByText(/1 predecessor, 2 successors within 2 levels/)).toBeTruthy();
    // The driving link is drawn in hi-vis; the lagged SS link says so.
    expect(container.querySelectorAll(".network-link.driving")).toHaveLength(1);
    expect(container.textContent).toContain("SS+8h");
    expect(screen.getByRole("button", { name: /^A1 .*critical/ })).toBeTruthy();
    act(() => {
      const now = view.current();
      if (now) view.publish({ ...now, active: 3 });
    });
    expect(screen.getByRole("heading").textContent).toBe("Network: A4");
  });

  it("goes as many levels each way as chosen", () => {
    render(<NetworkPanel view={viewOn(0)} links={LINKS} onClose={() => {}} />);
    expect(boxes()).toHaveLength(3);
    fireEvent.change(screen.getByLabelText("Levels each way"), { target: { value: "3" } });
    expect(boxes()).toHaveLength(4);
    fireEvent.change(screen.getByLabelText("Levels each way"), { target: { value: "1" } });
    expect(boxes().sort()).toEqual(["A1", "A2"]);
  });

  it("selects an activity in the grid when its box is clicked or pressed", () => {
    const view = viewOn(1);
    const reveal = vi.spyOn(view, "reveal");
    render(<NetworkPanel view={view} links={LINKS} onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /^A3 / }));
    expect(reveal).toHaveBeenCalledWith(3);
    fireEvent.keyDown(screen.getByRole("button", { name: /^A1 / }), { key: "Enter" });
    expect(reveal).toHaveBeenLastCalledWith(1);
  });

  it("asks for a selection, and waits for the links", () => {
    const empty = new View();
    render(<NetworkPanel view={empty} links={LINKS} onClose={() => {}} />);
    expect(screen.getByText(/Select an activity in the grid/)).toBeTruthy();
    cleanup();
    render(<NetworkPanel view={viewOn(0)} links={null} onClose={() => {}} />);
    expect(screen.getByText(/still on their way/)).toBeTruthy();
  });
});
