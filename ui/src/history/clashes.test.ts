import { describe, expect, it } from "vitest";

import type { HistoryDiff, Problem } from "../api";
import diffAnswer from "../api/golden/history-diff-1.json";
import { type Shown, clashOf, setting, sinceParting } from "./clashes";

const CLASH: Problem = {
  source: "project 7",
  code: "both-changed",
  message: "A2 original_duration: this branch has 64h, and the other has 56h",
  hint: "",
};

describe("a refused merge's clashes", () => {
  it("are read for the activity, project, field and both values", () => {
    expect(clashOf(CLASH)).toEqual({ code: "A2", project: 7, field: "original_duration", ours: "64h", theirs: "56h", message: CLASH.message });
    expect(clashOf({ ...CLASH, message: "A3 deleted: one branch deleted it, and the other changed it" })).toMatchObject({ code: "A3", field: null, ours: null });
    expect(clashOf({ ...CLASH, message: "link A1 FS A2: both branches made this change" }).code).toBeNull();
  });

  it("take values from the comparison of the heads, as the words can't always be split", () => {
    const words = { ...CLASH, message: "A2 name: this branch has Mobilise, and the other has Pour, and the other has Crane" };
    // Ambiguous on its own: no values guessed.
    expect(clashOf(words)).toMatchObject({ ours: null, theirs: null });
    const compared = structuredClone(diffAnswer) as unknown as HistoryDiff;
    const change = compared.diff.projects[0]?.changed?.[0];
    if (change) change.changes = [{ category: "identity", field: "name", from: "Mobilise", to: "Pour, and the other has Crane" }];
    expect(clashOf(words, compared, () => "rel-fs-positive")).toMatchObject({ ours: "Mobilise", theirs: "Pour, and the other has Crane" });
  });

  it("take values only from the clash's own project", () => {
    const compared = structuredClone(diffAnswer) as unknown as HistoryDiff;
    const change = compared.diff.projects[0]?.changed?.[0];
    if (change) change.changes = [{ category: "duration", field: "original_duration", from: "1h", to: "2h" }];
    // The comparison's A2 is in rel-fs-positive; the clash is in project 8, another.
    expect(clashOf({ ...CLASH, source: "project 8" }, compared, (id) => (id === 7 ? "rel-fs-positive" : "other"))).toMatchObject({
      ours: "64h",
      theirs: "56h",
    });
    expect(clashOf(CLASH, compared, (id) => (id === 7 ? "rel-fs-positive" : "other"))).toMatchObject({ ours: "1h", theirs: "2h" });
  });
});

/** A changeset as `history show` gives it, with its parents and edits. */
function doc(id: string, parents: [string | null, string | null], edits: Record<string, unknown>[] = [], project = 7): Shown {
  return {
    id,
    short_id: id,
    parent: parents[0],
    second_parent: parents[1],
    message: `change ${id}`,
    summary: `edits of ${id}`,
    commands: {
      edits,
      resolved: edits.map((e) => ({ kind: "activity", name: e.activity, project })),
    },
  } as Shown;
}

describe("the changes on each side since the branches parted", () => {
  it("follow merges, so a change merged earlier is shared, not incoming", async () => {
    // O is where they parted. Main has P, then merged F (which the other branch has); the other branch went on to T.
    const docs = new Map(
      [doc("O", [null, null]), doc("P", ["O", null]), doc("F", ["O", null]), doc("M", ["P", "F"]), doc("T", ["F", null])].map((d) => [d.id, d]),
    );
    const show = (id: string) => Promise.resolve(docs.get(id) as Shown);
    const sides = await sinceParting(
      show,
      { head: "M", known: new Set(["M", "P", "O"]) },
      { head: "T", known: new Set(["T", "F", "O"]) },
    );
    expect(sides.ours.map((d) => d.id).sort()).toEqual(["M", "P"]);
    expect(sides.theirs.map((d) => d.id)).toEqual(["T"]);
  });

  it("are the ones whose commands set the clash's field on its activity, in its project", () => {
    const clash = { code: "A2", project: 7, field: "original_duration", ours: null, theirs: null, message: "" };
    const docs = [
      doc("duration", [null, null], [{ op: "set_duration", activity: "A2", original: "5d" }]),
      // The fourth edit of a batch, which a summary leaves out.
      doc("batch", [null, null], [
        { op: "set_name", activity: "A1", name: "x" },
        { op: "set_name", activity: "A3", name: "y" },
        { op: "set_name", activity: "A4", name: "z" },
        { op: "set_duration", activity: "A2", original: "6d" },
      ]),
      doc("rename", [null, null], [{ op: "set_name", activity: "A2", name: "Pour" }]),
      doc("remaining", [null, null], [{ op: "set_duration", activity: "A2", remaining: "2d" }]),
      doc("other project", [null, null], [{ op: "set_duration", activity: "A2", original: "7d" }], 8),
      doc("A20", [null, null], [{ op: "set_duration", activity: "A20", original: "7d" }]),
    ];
    // A remaining set alone never sets original.
    expect(setting(docs, clash).map((d) => d.id)).toEqual(["duration", "batch"]);
    // An original set alone can set remaining too, which follows it on work not started.
    expect(setting(docs, { ...clash, field: "remaining_duration" }).map((d) => d.id)).toEqual(["duration", "batch", "remaining"]);
  });

  it("respect an edit's project, when it was made for one", () => {
    const clash = { code: "A2", project: 8, field: "original_duration", ours: null, theirs: null, message: "" };
    // A2 was found in both projects when it was recorded; the request was for project 8.
    const both = doc("for 8", [null, null], [{ op: "set_duration", activity: "A2", original: "5d" }]);
    (both.commands as Record<string, unknown>).resolved = [
      { kind: "activity", name: "A2", project: 7 },
      { kind: "activity", name: "A2", project: 8 },
    ];
    (both.commands as Record<string, unknown>).options = { project: 8 };
    expect(setting([both], clash).map((d) => d.id)).toEqual(["for 8"]);
    expect(setting([both], { ...clash, project: 7 })).toEqual([]);
  });
});
