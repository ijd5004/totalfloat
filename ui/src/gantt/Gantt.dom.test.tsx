// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { sample } from "../edit/fixture";
import { DEFAULT_ARRANGEMENT, arrange, buildModel } from "../grid/model";
import { View } from "../view";
import { Gantt, dragWords, hoverText, linkText, pick, spanShown } from "./Gantt";
import { type ZoomChoice, minutesOf, timesOf } from "./time";
import { DEFAULT_DATES, showDates } from "../dates";

afterEach(cleanup);

function shownFor(top = 0) {
  const rows = sample();
  return { rows, arranged: arrange(buildModel(rows), DEFAULT_ARRANGEMENT), top, height: 400, rowHeight: 22, active: 0, bodyTop: 60 };
}

describe("the Gantt", () => {
  it("follows the grid once it shows rows, and scrolls it with the wheel", () => {
    const view = new View();
    const asked: number[] = [];
    view.scrollWith((top) => asked.push(top));
    // Mounted before the grid has rows, as the app is.
    const { container } = render(<Gantt view={view} summary={null} zoom={3} onZoom={() => {}} today={false} />);
    expect(container.querySelector(".gantt-body")).toBeNull();
    act(() => view.publish(shownFor(44)));
    const body = container.querySelector(".gantt-body") as HTMLElement;
    expect(body).not.toBeNull();
    // Two notches before the grid draws again both count.
    fireEvent.wheel(body, { deltaY: 100 });
    fireEvent.wheel(body, { deltaY: 100 });
    expect(asked).toEqual([100, 100]);
  });

  it("zooms with Ctrl and the wheel, rather than scrolling", () => {
    const view = new View();
    const asked: number[] = [];
    const zooms: ZoomChoice[] = [];
    view.scrollWith((top) => asked.push(top));
    const { container } = render(
      <Gantt view={view} summary={null} zoom={3} onZoom={(zoom) => zooms.push(zoom)} today={false} />,
    );
    act(() => view.publish(shownFor()));
    const body = container.querySelector(".gantt-body") as HTMLElement;
    fireEvent.wheel(body, { deltaY: -100, ctrlKey: true });
    fireEvent.wheel(body, { deltaY: 100, ctrlKey: true });
    expect(zooms).toEqual([4, 2]);
    expect(asked).toEqual([]);
  });

  it("needs a notch's worth of a pinch to zoom a level", () => {
    const view = new View();
    const zooms: ZoomChoice[] = [];
    const { container } = render(<Gantt view={view} summary={null} zoom={3} onZoom={(z) => zooms.push(z)} today={false} />);
    act(() => view.publish(shownFor()));
    const body = container.querySelector(".gantt-body") as HTMLElement;
    for (let n = 0; n < 9; n++) fireEvent.wheel(body, { deltaY: -5, ctrlKey: true });
    expect(zooms).toEqual([]);
    fireEvent.wheel(body, { deltaY: -5, ctrlKey: true });
    expect(zooms).toEqual([4]);
  });

  it("scrolls the grid by lines when the wheel counts in lines", () => {
    const view = new View();
    const asked: number[] = [];
    view.scrollWith((top) => asked.push(top));
    const { container } = render(<Gantt view={view} summary={null} zoom={3} onZoom={() => {}} today={false} />);
    act(() => view.publish(shownFor()));
    fireEvent.wheel(container.querySelector(".gantt-body") as HTMLElement, { deltaY: 3, deltaMode: 1 });
    expect(asked).toEqual([66]);
  });

  it("keeps the moment in the middle where it was when the zoom changes, from any source", () => {
    const view = new View();
    const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
    try {
      const props = { view, summary: null, onZoom: () => {}, today: false };
      const { container, rerender } = render(<Gantt {...props} zoom={3} />);
      act(() => view.publish(shownFor()));
      const body = container.querySelector(".gantt-body") as HTMLElement;
      body.scrollLeft = 900;
      fireEvent.scroll(body);
      const extent = parseFloat((container.querySelector(".gantt-extent") as HTMLElement).style.width);
      rerender(<Gantt {...props} zoom={4} />);
      const after = parseFloat((container.querySelector(".gantt-extent") as HTMLElement).style.width);
      // Three times the pixels a day, so the middle's distance from the axis
      // start triples, and the scroll moves to keep it in the middle.
      expect(after / extent).toBeCloseTo(3, 3);
      expect(body.scrollLeft).toBeCloseTo((900 + 300) * 3 - 300, 3);
    } finally {
      if (width) Object.defineProperty(HTMLElement.prototype, "clientWidth", width);
    }
  });

  it("keeps the middle's moment when zooming out, though the browser clamped the scroll first", () => {
    const view = new View();
    const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
    try {
      const props = { view, summary: null, onZoom: () => {}, today: false };
      const { container, rerender } = render(<Gantt {...props} zoom={3} />);
      act(() => view.publish(shownFor()));
      const body = container.querySelector(".gantt-body") as HTMLElement;
      body.scrollLeft = 2000;
      fireEvent.scroll(body);
      // A narrower axis can clamp the scroll before the chart hears of the zoom.
      body.scrollLeft = 100;
      rerender(<Gantt {...props} zoom={2} />);
      // From 8 to 3 pixels a day: the middle, 2,300 px in, is now 862.5 px in.
      expect(body.scrollLeft).toBeCloseTo(2300 * (3 / 8) - 300, 3);
    } finally {
      if (width) Object.defineProperty(HTMLElement.prototype, "clientWidth", width);
    }
  });

  it("keeps the middle as it is after a resize, not as it was, when the zoom changes", () => {
    const view = new View();
    const descriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    let width = 600;
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => width });
    try {
      const props = { view, summary: null, onZoom: () => {}, today: false };
      const { container, rerender } = render(<Gantt {...props} zoom={3} />);
      act(() => view.publish(shownFor()));
      const body = container.querySelector(".gantt-body") as HTMLElement;
      body.scrollLeft = 900;
      fireEvent.scroll(body);
      // Widened with the splitter: no scroll, just a redraw.
      width = 900;
      act(() => view.publish(shownFor(1)));
      rerender(<Gantt {...props} zoom={4} />);
      // The middle is now 1,350 px in; at three times the scale it stays in the middle.
      expect(body.scrollLeft).toBeCloseTo(1350 * 3 - 450, 3);
    } finally {
      if (descriptor) Object.defineProperty(HTMLElement.prototype, "clientWidth", descriptor);
    }
  });

  it("brings a revealed activity's bar into view across time", () => {
    const view = new View();
    const width = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 300 });
    try {
      const { container } = render(<Gantt view={view} summary={null} zoom={3} onZoom={() => {}} today={false} />);
      act(() => view.publish(shownFor()));
      const body = container.querySelector(".gantt-body") as HTMLElement;
      // Scrolled far past A1's bar, which starts about a month into the axis.
      body.scrollLeft = 5000;
      act(() => view.reveal(1));
      expect(body.scrollLeft).toBeLessThan(5000);
      expect(body.scrollLeft).toBeGreaterThan(0);
      // Revealing it again, now in view, leaves the scroll alone.
      const now = body.scrollLeft;
      act(() => view.reveal(1));
      expect(body.scrollLeft).toBe(now);
    } finally {
      if (width) Object.defineProperty(HTMLElement.prototype, "clientWidth", width);
    }
  });

  it("drops the tooltip when the rows scroll under a still pointer", () => {
    const view = new View();
    const { container } = render(<Gantt view={view} summary={null} zoom={3} onZoom={() => {}} today={false} />);
    act(() => view.publish(shownFor()));
    const body = container.querySelector(".gantt-body") as HTMLElement;
    // Line 1 is A1, under the band.
    fireEvent.mouseMove(body, { clientX: 100, clientY: 30 });
    expect(container.querySelector(".gantt-tip")?.textContent).toContain("A1");
    act(() => view.publish(shownFor(22)));
    expect(container.querySelector(".gantt-tip")).toBeNull();
  });

  it("names the link a drag would add before it's released", () => {
    const rows = sample();
    const from = { id: 1, end: "finish" as const, x: 0, y: 0 };
    expect(dragWords(rows, { from, to: null })).toEqual(["From A1's finish", "Drop on another bar's start or finish. Escape cancels."]);
    expect(dragWords(rows, { from, to: { id: 2, end: "start", x: 0, y: 0 } })[0]).toBe("Link A1 to A2, finish to start");
    expect(dragWords(rows, { from: { ...from, end: "start" }, to: { id: 2, end: "start", x: 0, y: 0 } })[0]).toBe("Link A1 to A2, start to start");
  });

  it("selects the activity whose bar is clicked", () => {
    const view = new View();
    const revealed: number[] = [];
    view.revealWith((id) => revealed.push(id));
    const { container } = render(<Gantt view={view} summary={null} zoom={3} onZoom={() => {}} today={false} />);
    act(() => view.publish(shownFor()));
    // Line 2 is A2.
    fireEvent.click(container.querySelector(".gantt-body") as HTMLElement, { clientX: 100, clientY: 50 });
    expect(revealed).toEqual([2]);
  });

  it("picks a short bar over a line passing across it, and an arrow over both", () => {
    // A2's bar is three pixels wide, on its row (22 to 44), with a line across it.
    const drawn = {
      lines: [{ points: [[0, 33], [40, 33]] as [number, number][], link: 0 }],
      arrows: [{ x: 60, y: 33, to: 9 }],
      bars: [{ left: 10, right: 13, top: 22, bottom: 44, id: 2 }],
    };
    expect(pick(drawn, 11, 33, 1)).toEqual({ activity: 1 });
    // Off the bar, on the line: the line, and the row it's in.
    expect(pick(drawn, 20, 33, 1)).toEqual({ link: 0, row: 1 });
    expect(pick(drawn, 58, 33, 1)).toMatchObject({ arrow: { to: 9 } });
    expect(pick(drawn, 20, 40, 1)).toEqual({ activity: 1 });
  });

  it("says what a line joins, its type and lag, and whether it drives", () => {
    const rows = sample();
    const links = { count: 1, id: [9], predecessor: [1], successor: [2], type: ["SS"], lag_hours: [8], driving: [true] } as unknown as import("../api").Links;
    const lineOf = new Map([
      [1, 1],
      [2, 2],
    ]);
    expect(linkText(rows, links, 0, lineOf, Int32Array.from([-1, 0, 1, 2]))).toEqual(["A1 → A2", "SS, lag 8 h, driving"]);
  });

  it("hovers with float in days of the activity's calendar", () => {
    const rows = sample({ hours_per_day: [8, 8, 8], total_float_hours: [16, 0, null] });
    expect(hoverText(rows, 0).at(-1)).toBe("Total float: 2 days");
  });

  it("hovers with the rows' own dates, in the format shown", () => {
    const rows = sample({ late_start: ["2026-01-08 08:00", null, null], late_finish: ["2026-01-12 17:00", null, null], total_float_hours: [16, 0, null] });
    expect(hoverText(rows, 0)).toEqual([
      "A1 · Name A1",
      "Early: 05 Jan 26 to 09 Jan 26",
      "Late: 08 Jan 26 to 12 Jan 26",
      "Total float: 16 h",
    ]);
    showDates({ style: "yyyy-mm-dd", time: true });
    expect(hoverText(rows, 0)[1]).toBe("Early: 2026-01-05 08:00 to 2026-01-09 17:00");
    showDates(DEFAULT_DATES);
  });
});

describe("what Fit fits", () => {
  it("runs from the first start to the last finish shown, out to a late finish where float runs past", () => {
    const rows = sample({
      early_start: ["2026-01-05 08:00", "2026-01-12 08:00", "2026-02-02 08:00"],
      early_finish: ["2026-01-09 17:00", "2026-01-16 17:00", "2026-02-06 17:00"],
      late_finish: ["2026-01-30 17:00", null, null],
    });
    const times = timesOf(rows);
    const span = (lines: number[]) => spanShown(times, { lines: Int32Array.from(lines), bands: [], matched: lines.length });
    expect(span([0, 1, 2])).toEqual({ first: minutesOf("2026-01-05 08:00"), last: minutesOf("2026-02-06 17:00") });
    // Filtered to the first two: the first's float runs to 30 Jan.
    expect(span([0, 1])).toEqual({ first: minutesOf("2026-01-05 08:00"), last: minutesOf("2026-01-30 17:00") });
    expect(span([1])).toEqual({ first: minutesOf("2026-01-12 08:00"), last: minutesOf("2026-01-16 17:00") });
    expect(span([])).toBeNull();
  });
});
