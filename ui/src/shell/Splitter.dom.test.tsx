// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Splitter } from "./Shell";

afterEach(cleanup);

describe("the splitter", () => {
  function setup() {
    const splits: number[] = [];
    const done: number[] = [];
    const { container } = render(
      <div className="views">
        <Splitter split={0.6} onSplit={(s) => splits.push(s)} onDone={(s) => done.push(s)} />
      </div>,
    );
    const views = container.querySelector(".views") as HTMLElement;
    views.getBoundingClientRect = () => ({ left: 0, width: 1000, top: 0, height: 500, right: 1000, bottom: 500, x: 0, y: 0, toJSON: () => ({}) });
    return { bar: container.querySelector(".splitter") as HTMLElement, splits, done };
  }

  it("follows a drag, and keeps the split once, when it ends or is cancelled", () => {
    const { bar, splits, done } = setup();
    fireEvent.pointerDown(bar, { button: 0, pointerId: 1, clientX: 600 });
    fireEvent.pointerMove(bar, { clientX: 400 });
    fireEvent.pointerCancel(bar);
    // Moves after the browser took the pointer back change nothing.
    fireEvent.pointerMove(bar, { clientX: 200 });
    expect(splits).toEqual([0.4]);
    expect(done).toEqual([0.4]);
  });

  it("ignores buttons other than the main one", () => {
    const { bar, splits, done } = setup();
    fireEvent.pointerDown(bar, { button: 2, pointerId: 1, clientX: 600 });
    fireEvent.pointerMove(bar, { clientX: 400 });
    expect(splits).toEqual([]);
    expect(done).toEqual([]);
  });

  it("moves with the arrow keys, within bounds", () => {
    const { bar, done } = setup();
    fireEvent.keyDown(bar, { key: "ArrowLeft" });
    expect(done).toHaveLength(1);
    expect(done[0]).toBeCloseTo(0.55, 9);
  });
});
