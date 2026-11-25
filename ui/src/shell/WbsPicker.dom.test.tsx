// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { WbsPicker, wbsChoices } from "./WbsPicker";

afterEach(cleanup);

const ORDER = new Map([
  ["P", 0],
  ["P.2", 1],
  ["P.1", 2],
  ["P.1.A", 3],
]);
const NAMES = new Map([
  ["P.1", "Civil"],
  ["P.2", "Yard"],
]);

describe("the WBS picker", () => {
  it("lists every node in the WBS's order, each under its parent", () => {
    expect(wbsChoices(ORDER, NAMES).map((c) => [c.path, c.depth])).toEqual([
      ["P", 0],
      ["P.2", 1],
      ["P.1", 1],
      ["P.1.A", 2],
    ]);
  });

  it("filters by code or name, moves with the arrows, and picks with Enter", () => {
    const onPick = vi.fn();
    const onClose = vi.fn();
    render(<WbsPicker title="Move A5 to which WBS node?" choices={wbsChoices(ORDER, NAMES)} onPick={onPick} onClose={onClose} />);
    const find = screen.getByRole("combobox");
    expect(document.activeElement).toBe(find);
    fireEvent.change(find, { target: { value: "civ" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["CivilP.1"]);
    fireEvent.change(find, { target: { value: "P.1" } });
    fireEvent.keyDown(find, { key: "ArrowDown" });
    expect(screen.getAllByRole("option")[1]?.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(find, { key: "Enter" });
    expect(onPick).toHaveBeenCalledWith("P.1.A");
    fireEvent.keyDown(find, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
