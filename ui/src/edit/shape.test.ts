import { describe, expect, it } from "vitest";

import type { Change } from "./editing";
import { sample } from "./fixture";
import type { WbsRef } from "../grid/model";
import { type NewActivity, addEdit, changeKey, rowsWith, wbsProblem, wbsWith } from "./shape";

const typed = (change: Partial<NewActivity> = {}): NewActivity => ({ code: "A10", name: "Survey", wbs: "P", duration: "", ...change });

describe("an activity typed into a new row", () => {
  it("becomes an add_activity edit, in the units typed or shown", () => {
    expect(addEdit(typed({ duration: "3" }), "P", "d")).toEqual({ op: "add_activity", code: "A10", name: "Survey", wbs: "P", duration: "3d" });
    expect(addEdit(typed({ duration: "16h" }), "P", "d")).toEqual({ op: "add_activity", code: "A10", name: "Survey", wbs: "P", duration: "16h" });
    expect(addEdit(typed({ duration: "16" }), "P", "h")).toEqual({ op: "add_activity", code: "A10", name: "Survey", wbs: "P", duration: "16h" });
  });

  it("trims the ID and name", () => {
    expect(addEdit(typed({ code: " A10 ", name: " Survey ", duration: "1" }), "P", "d")).toEqual({ op: "add_activity", code: "A10", name: "Survey", wbs: "P", duration: "1d" });
  });

  it("adds a milestone for a duration of 0", () => {
    expect(addEdit(typed({ duration: "0" }), "P", "d")).toEqual({
      op: "add_activity",
      code: "A10",
      name: "Survey",
      wbs: "P",
      activity_type: "finish-milestone",
    });
    expect(addEdit(typed({ duration: "0h" }), "P", "d")).toMatchObject({ activity_type: "finish-milestone" });
  });

  it("says what's missing or wrong", () => {
    expect(addEdit(typed({ code: " " }), "P", "d")).toBe("type an Activity ID");
    expect(addEdit(typed({ name: "" }), "P", "d")).toBe("type a name");
    // Work needs a duration; the engine refuses it without one.
    expect(addEdit(typed({ duration: " " }), "P", "d")).toBe("type a duration, or 0 for a milestone");
    expect(typeof addEdit(typed({ duration: "soon" }), "P", "d")).toBe("string");
  });
});

describe("rows with the changes waiting", () => {
  const add = (activity: NewActivity): Change => {
    const edit = addEdit(activity, activity.wbs, "d");
    if (typeof edit === "string") throw new Error(edit);
    return { key: changeKey("add", activity.code), edit, words: `Add ${activity.code}`, adds: activity };
  };

  it("adds each activity last, with a negative id and no dates, and leaves out those deleted", () => {
    const rows = rowsWith(
      sample(),
      [
        add(typed({ code: "A10", duration: "2" })),
        { key: changeKey("delete", "A2"), edit: { op: "delete_activity", activity: "A2" }, words: "Delete A2" },
        add(typed({ code: "A20", name: "Ready", duration: "0" })),
      ],
      "d",
    );
    expect(rows.count).toBe(4);
    expect(rows.code).toEqual(["A1", "A3", "A10", "A20"]);
    expect(rows.id).toEqual([1, 3, -1, -2]);
    expect(rows.type).toEqual(["task", "task", "task", "finish-milestone"]);
    expect(rows.original_duration_hours).toEqual([40, 40, 16, 0]);
    expect(rows.early_start).toEqual(["2026-01-05 08:00", "2026-01-05 08:00", null, null]);
    expect(rows.project).toEqual([7, 7, 7, 7]);
    expect(rows.wbs_path).toEqual(["P", "P", "P", "P"]);
  });

  it("gives the same rows back when nothing changes them", () => {
    const rows = sample();
    expect(rowsWith(rows, [], "d")).toBe(rows);
    expect(rowsWith(rows, undefined, "d")).toBe(rows);
  });

  it("deletes an ID in the project the change names, where IDs repeat across projects", () => {
    const rows = rowsWith(
      sample({ code: ["A1", "A2", "A2"], project: [7, 7, 8] }),
      [{ key: changeKey("delete", "A2"), edit: { op: "delete_activity", activity: "A2" }, words: "Delete A2", project: 8 }],
      "d",
    );
    expect(rows.id).toEqual([1, 2]);
  });
});

describe("WBS changes waiting", () => {
  const refs = (): Map<string, WbsRef> =>
    new Map([
      ["P", { project: 7, code: "P", id: 1, ref: "P", projectNode: true }],
      ["P.1", { project: 7, code: "1", id: 2, ref: "1" }],
      ["P.2", { project: 7, code: "2", id: 3, ref: "2" }],
    ]);
  const shown = () => ({ order: new Map([["P", 0], ["P.1", 1], ["P.2", 2]]), names: new Map([["P.1", "Civil"]]), refs: refs() });

  it("adds a node after its siblings with its name, and renames another, keeping its path", () => {
    const after = wbsWith(shown(), [
      { key: "a", edit: { op: "add_wbs", code: "3", name: "Survey", parent: "P" }, words: "", project: 7, wbs: { path: "P.3", code: "3", name: "Survey" } },
      { key: "r", edit: { op: "rename_wbs", wbs: "1", name: "Civil works" }, words: "", project: 7, wbs: { path: "P.1", name: "Civil works" } },
    ]);
    expect([...(after.order ?? [])].sort((a, b) => a[1] - b[1]).map(([path]) => path)).toEqual(["P", "P.1", "P.2", "P.3"]);
    expect(after.names?.get("P.3")).toBe("Survey");
    expect(after.names?.get("P.1")).toBe("Civil works");
    // A node not yet added has no ID: edits name it by its code.
    expect(after.refs?.get("P.3")).toEqual({ project: 7, code: "3", ref: "3" });
  });

  it("names a node by its ID once another takes its code, as a new node or a new code does", () => {
    const after = wbsWith(shown(), [
      { key: "a", edit: { op: "add_wbs", code: "1", name: "Again", parent: "2" }, words: "", project: 7, wbs: { path: "P.2.1", code: "1", name: "Again" } },
      { key: "r", edit: { op: "rename_wbs", wbs: "2", code: "X" }, words: "", project: 7, wbs: { path: "P.2", code: "X" } },
    ]);
    expect(after.refs?.get("P.1")?.ref).toBe("2");
    // The new node's code isn't its own alone, and it has no ID yet.
    expect(after.refs?.get("P.2.1")?.ref).toBeNull();
    expect(after.refs?.get("P.2")?.ref).toBe("X");
  });

  it("gives the same WBS back with no WBS changes", () => {
    const before = shown();
    expect(wbsWith(before, [])).toBe(before);
  });

  it("refuses a code or name missing, a dot, or a code a sibling has", () => {
    expect(wbsProblem(refs(), "", "Survey", "P", null)).toBe("type a code");
    expect(wbsProblem(refs(), "3", " ".trim(), "P", null)).toBe("type a name");
    expect(wbsProblem(refs(), "3.1", "Survey", "P", null)).toContain("can't hold a dot");
    expect(wbsProblem(refs(), "1", "Survey", "P", null)).toBe("1 is already under that node");
    // Its own code is no clash when it's renamed.
    expect(wbsProblem(refs(), "1", "Civil", "P", "P.1")).toBeNull();
    expect(wbsProblem(refs(), "3", "Survey", "P", null)).toBeNull();
  });

  it("shows an activity moved under its new node", () => {
    const rows = rowsWith(
      sample({ wbs_path: ["P.1", "P.1", "P.2"] }),
      [{ key: changeKey("move", "A2"), edit: { op: "set_wbs", activity: "A2", wbs: "2" }, words: "", project: 7, moves: "P.2" }],
      "d",
    );
    expect(rows.wbs_path).toEqual(["P.1", "P.2", "P.2"]);
  });
});
