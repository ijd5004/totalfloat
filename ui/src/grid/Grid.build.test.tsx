// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { CellKey, CellProblem } from "../edit/cells";
import { sample } from "../edit/fixture";
import type { NewActivity } from "../edit/shape";
import { Grid, type GridActions, type GridEditing, type WbsTyped } from "./Grid";

afterEach(cleanup);

const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
});
afterAll(() => {
  if (height) Object.defineProperty(HTMLElement.prototype, "clientHeight", height);
});

/** Editing that can add, offering A10, A20, ... as activities are added. */
function building(refuse: string | null = null) {
  const added: NewActivity[] = [];
  const editing: GridEditing = {
    enabled: true,
    values: new Map<CellKey, string>(),
    marks: new Map<CellKey, CellProblem>(),
    previewed: false,
    commit: vi.fn(),
    apply: vi.fn(),
    discard: vi.fn(),
    say: vi.fn(),
    isShortcut: (event) => event.key === "?" || event.key === "/",
    add: (activity) => {
      if (refuse) return refuse;
      added.push(activity);
      return null;
    },
    nextId: (wbs) => `${wbs}${(added.length + 1) * 10}`,
  };
  return { editing, added };
}

// Lines: 0 band P, 1-3 A1-A3, 4 P's Add activity, 5 band Q (empty), 6 Q's Add activity.
function grid(editing: GridEditing, actions?: { current: GridActions | null }) {
  const rendered = render(
    <Grid rows={sample()} editing={editing} bandOrder={new Map([["P", 0], ["Q", 1]])} {...(actions ? { actions } : {})} />,
  );
  const body = rendered.container.querySelector(".grid-body") as HTMLElement;
  body.focus();
  return { ...rendered, body };
}

const down = (body: HTMLElement, times: number) => {
  for (let k = 0; k < times; k++) fireEvent.keyDown(body, { key: "ArrowDown" });
};
const input = (container: HTMLElement, label: string) => container.querySelector(`[aria-label="${label}"]`) as HTMLInputElement;

describe("adding activities in the grid", () => {
  it("ends each WBS band with an Add activity line, empty bands too", () => {
    const { container } = grid(building().editing);
    const lines = [...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => tr.textContent);
    expect(lines).toHaveLength(7);
    expect(lines[4]).toBe("Add an activity to WBS P: Enter adds one");
    expect(lines[5]).toBe("WBS Q, 0 activities");
    expect(lines[6]).toBe("Add an activity to WBS Q: Enter adds one");
  });

  it("opens a new row on Enter with the ID offered and the name focused, and Enter adds it and opens the next", () => {
    const { editing, added } = building();
    const { container, body } = grid(editing);
    down(body, 6);
    fireEvent.keyDown(body, { key: "Enter" });
    expect(input(container, "New activity's ID").value).toBe("Q10");
    const name = input(container, "New activity's name");
    expect(document.activeElement).toBe(name);
    fireEvent.change(name, { target: { value: "Survey" } });
    fireEvent.change(input(container, "New activity's duration"), { target: { value: "2" } });
    fireEvent.keyDown(name, { key: "Enter" });
    expect(added).toEqual([{ wbs: "Q", code: "Q10", name: "Survey", duration: "2" }]);
    // The next row: the next ID, the name empty and focused.
    expect(input(container, "New activity's ID").value).toBe("Q20");
    expect(input(container, "New activity's name").value).toBe("");
    expect(document.activeElement).toBe(input(container, "New activity's name"));
    fireEvent.change(input(container, "New activity's ID"), { target: { value: "X1" } });
    fireEvent.change(input(container, "New activity's name"), { target: { value: "Ready" } });
    fireEvent.change(input(container, "New activity's duration"), { target: { value: "0" } });
    fireEvent.keyDown(input(container, "New activity's duration"), { key: "Enter" });
    expect(added[1]).toEqual({ wbs: "Q", code: "X1", name: "Ready", duration: "0" });
    // Escape closes it and gives the grid its focus back.
    fireEvent.keyDown(input(container, "New activity's name"), { key: "Escape" });
    expect(container.querySelector(".new-row")).toBeNull();
    expect(document.activeElement).toBe(body);
    expect(added).toHaveLength(2);
  });

  it("opens with the letter typed on an Add activity line as the name", () => {
    const { editing } = building();
    const { container, body } = grid(editing);
    down(body, 4);
    fireEvent.keyDown(body, { key: "S" });
    expect(input(container, "New activity's name").value).toBe("S");
    expect(input(container, "New activity's ID").value).toBe("P10");
  });

  it("keeps the row open and says why when the activity can't be added", () => {
    const { editing } = building("type a name");
    const { container, body } = grid(editing);
    down(body, 4);
    fireEvent.keyDown(body, { key: "Enter" });
    fireEvent.keyDown(input(container, "New activity's name"), { key: "Enter" });
    expect(container.querySelector(".new-row-problem")?.textContent).toBe("type a name");
    expect(container.querySelector(".new-row")).not.toBeNull();
  });

  it("adds in the selected activity's WBS node when the frame asks, and gives the activities selected", () => {
    const actions: { current: GridActions | null } = { current: null };
    const { container, body } = grid(building().editing, actions);
    down(body, 2);
    fireEvent.keyDown(body, { key: "ArrowDown", shiftKey: true });
    expect(actions.current?.selected()).toEqual([2, 3]);
    act(() => actions.current?.addHere());
    expect(input(container, "New activity's ID").value).toBe("P10");
  });

  it("opens a collapsed band when the frame asks to add in it", () => {
    const actions: { current: GridActions | null } = { current: null };
    const { container, body } = grid(building().editing, actions);
    down(body, 1);
    // Collapse P from its band's line, then ask from there.
    fireEvent.keyDown(body, { key: "ArrowUp" });
    fireEvent.keyDown(body, { key: "ArrowLeft" });
    expect([...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => tr.textContent)[0]).toContain("collapsed");
    act(() => actions.current?.addHere());
    expect(input(container, "New activity's ID").value).toBe("P10");
  });

  it("doesn't edit the cells of an activity waiting to be added, and says why", () => {
    const { editing } = building();
    const said: string[] = [];
    editing.say = (text) => void said.push(text);
    const rows = sample({ id: [1, 2, -1], code: ["A1", "A2", "A10"] });
    const { container } = render(<Grid rows={rows} editing={editing} bandOrder={new Map([["P", 0]])} />);
    const body = container.querySelector(".grid-body") as HTMLElement;
    body.focus();
    down(body, 3);
    fireEvent.keyDown(body, { key: "ArrowRight" });
    fireEvent.keyDown(body, { key: "S" });
    expect(container.querySelector(".cell-input")).toBeNull();
    expect(said).toEqual(["A10 is added once you apply the edits waiting; drop it and add it again to change it."]);
  });

  it("shows no Add activity lines when editing can't add", () => {
    const { editing } = building();
    const plain: GridEditing = { ...editing };
    delete plain.add;
    const { container } = grid(plain);
    const lines = [...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => tr.textContent);
    expect(lines.some((line) => line?.startsWith("Add an activity"))).toBe(false);
  });
});

describe("WBS nodes in the grid", () => {
  function wbsEditing(refuse: string | null = null) {
    const { editing } = building();
    const added: [string, WbsTyped][] = [];
    const renamed: [string, WbsTyped][] = [];
    editing.addWbs = (parent, node) => (refuse ?? (added.push([parent, node]), null));
    editing.renameWbs = (path, node) => (refuse ?? (renamed.push([path, node]), null));
    return { editing, added, renamed };
  }

  it("adds a node under the selected activity's, typed on a line after its child nodes", () => {
    const actions: { current: GridActions | null } = { current: null };
    const { editing, added } = wbsEditing();
    const { container, body } = grid(editing, actions);
    down(body, 1);
    act(() => actions.current?.addWbsHere());
    const lines = [...container.querySelectorAll("#grid-mirror tbody tr")].map((tr) => tr.textContent);
    // P has no child nodes: the new one goes before its activities.
    expect(lines[1]).toBe("New WBS node under WBS P: type its code and name");
    const code = input(container, "New WBS node's code");
    expect(document.activeElement).toBe(code);
    fireEvent.change(code, { target: { value: "3" } });
    fireEvent.change(input(container, "New WBS node's name"), { target: { value: "Survey" } });
    fireEvent.keyDown(code, { key: "Enter" });
    expect(added).toEqual([["P", { code: "3", name: "Survey" }]]);
    expect(container.querySelector(".wbs-row")).toBeNull();
    expect(document.activeElement).toBe(body);
  });

  it("renames a node with F2 on its band, starting from its code and name", () => {
    const { editing, renamed } = wbsEditing();
    const { container } = render(<Grid rows={sample()} editing={editing} bandOrder={new Map([["P", 0]])} bandNames={new Map([["P", "Civil"]])} />);
    const grid = container.querySelector(".grid-body") as HTMLElement;
    grid.focus();
    fireEvent.keyDown(grid, { key: "F2" });
    const name = input(container, "WBS node's name");
    expect(document.activeElement).toBe(name);
    expect(input(container, "WBS node's code").value).toBe("P");
    expect(name.value).toBe("Civil");
    fireEvent.change(name, { target: { value: "Civil works" } });
    fireEvent.keyDown(name, { key: "Enter" });
    expect(renamed).toEqual([["P", { code: "P", name: "Civil works" }]]);
  });

  it("closes the new node's line when a new activity row opens", () => {
    const actions: { current: GridActions | null } = { current: null };
    const { editing } = wbsEditing();
    const { container, body } = grid(editing, actions);
    down(body, 1);
    act(() => actions.current?.addWbsHere());
    expect(container.querySelector(".wbs-row")).not.toBeNull();
    act(() => actions.current?.addHere());
    expect(container.querySelector(".wbs-row")).toBeNull();
    expect(container.querySelectorAll(".new-row")).toHaveLength(1);
  });

  it("says a read-only schedule's nodes can't be renamed", () => {
    const { editing } = wbsEditing();
    const said: string[] = [];
    const { container } = render(<Grid rows={sample()} editing={{ ...editing, enabled: false, say: (text) => void said.push(text) }} bandOrder={new Map([["P", 0]])} />);
    const grid = container.querySelector(".grid-body") as HTMLElement;
    grid.focus();
    fireEvent.keyDown(grid, { key: "F2" });
    expect(said).toEqual(["This schedule is read-only here."]);
    expect(container.querySelector(".wbs-row")).toBeNull();
  });

  it("starts a rename from the code waiting, though the band keeps its path", () => {
    const { editing, renamed } = wbsEditing();
    editing.wbsNode = (path) => (path === "P" ? { code: "X", name: "Civil" } : undefined);
    const { container } = render(<Grid rows={sample()} editing={editing} bandOrder={new Map([["P", 0]])} bandNames={new Map([["P", "Civil"]])} />);
    const grid = container.querySelector(".grid-body") as HTMLElement;
    grid.focus();
    fireEvent.keyDown(grid, { key: "F2" });
    expect(input(container, "WBS node's code").value).toBe("X");
    const name = input(container, "WBS node's name");
    fireEvent.change(name, { target: { value: "Civil works" } });
    fireEvent.keyDown(name, { key: "Enter" });
    expect(renamed).toEqual([["P", { code: "X", name: "Civil works" }]]);
  });

  it("keeps the line open and says why when the node can't be added, and Escape closes it", () => {
    const actions: { current: GridActions | null } = { current: null };
    const { editing } = wbsEditing("type a code");
    const { container, body } = grid(editing, actions);
    act(() => actions.current?.addWbsHere());
    fireEvent.keyDown(input(container, "New WBS node's code"), { key: "Enter" });
    expect(container.querySelector(".new-row-problem")?.textContent).toBe("type a code");
    fireEvent.keyDown(input(container, "New WBS node's code"), { key: "Escape" });
    expect(container.querySelector(".wbs-row")).toBeNull();
    expect(document.activeElement).toBe(body);
  });
});
