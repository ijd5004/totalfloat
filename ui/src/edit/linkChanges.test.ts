// Link edits in the pending-edits flow: one per key, and a refusal on the
// edit it names, in words for a change rather than a cell.
import { describe, expect, it } from "vitest";

import { Api } from "../api";
import { addLink, changeLink, removeLink } from "../inspector/linkEdits";
import type { CellKey } from "./cells";
import { Editing, changesWaiting } from "./editing";
import { sample } from "./fixture";

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

const REPORT = {
  schema: "totalfloat.edit/1",
  file: "plant.tfdb@main",
  output: null,
  applied: [],
  warnings: [],
  projects: [{ id: 7, code: "P", scheduled_finish: "2026-01-16 17:00", previous_finish: "2026-01-09 17:00", changed_dates_or_float: 2, became_critical_total: 0, left_critical_total: 0 }],
};

function editing(...answers: ((body: Record<string, unknown>) => Response)[]) {
  const asked: Record<string, unknown>[] = [];
  const api = new Api("token", (_input, init) => {
    const body = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>;
    asked.push(body);
    const answer = answers.shift();
    if (!answer) throw new Error("nothing to answer");
    return Promise.resolve(answer(body));
  });
  const editor = new Editing(api, null);
  editor.show({ rows: sample(), head: "h1", writable: true });
  return { editor, asked };
}

const LINK = { link: 11, predecessor: 1, successor: 2, type: "FS" as const };

describe("link edits waiting", () => {
  it("keeps one edit per link: a second change to it takes the first's place", async () => {
    const { editor, asked } = editing(
      () => json(REPORT),
      () => json(REPORT),
    );
    const rows = sample();
    const retyped = changeLink(rows, LINK, undefined, { type: "SS" });
    if (!retyped) throw new Error("no change");
    await editor.change([retyped]);
    const both = changeLink(rows, LINK, retyped, { lag: "2d" });
    if (!both) throw new Error("no change");
    await editor.change([both]);
    expect(editor.state().pending?.changes?.map((c) => c.edit)).toEqual([
      { op: "change_link", predecessor: "A1", successor: "A2", link_type: "FS", new_type: "SS", lag: "2d" },
    ]);
    expect(asked[1]?.edits).toEqual([{ op: "change_link", predecessor: "A1", successor: "A2", link_type: "FS", new_type: "SS", lag: "2d" }]);
    // Removing it takes the change's place too.
    await editor.change([removeLink(rows, LINK)]);
    expect(editor.state().pending?.changes?.map((c) => c.edit.op)).toEqual(["remove_link"]);
  });

  it("keeps one per link among changes queued behind a preview, so they drain as one change_link", async () => {
    let release: () => void = () => {};
    const asked: Record<string, unknown>[] = [];
    const api = new Api("token", (_input, init) => {
      asked.push(JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>);
      // The first preview waits until released; the rest answer at once.
      if (asked.length === 1) return new Promise((done) => (release = () => done(json(REPORT))));
      return Promise.resolve(json(REPORT));
    });
    const editor = new Editing(api, null);
    editor.show({ rows: sample(), head: "h1", writable: true });
    const rows = sample();
    const first = editor.commit([{ id: 3, column: "name", text: "Pour" }]);
    const retyped = changeLink(rows, LINK, undefined, { type: "SS" });
    if (!retyped) throw new Error("no change");
    await editor.change([retyped]);
    // Built on the queued one, as the inspector builds it from what's waiting.
    const waiting = changesWaiting(editor.state()).find((c) => c.key === retyped.key);
    expect(waiting?.edit).toEqual(retyped.edit);
    const both = changeLink(rows, LINK, waiting, { lag: "2d" });
    if (!both) throw new Error("no change");
    await editor.change([both]);
    expect(changesWaiting(editor.state())).toHaveLength(1);
    release();
    await first;
    await new Promise((done) => setTimeout(done, 0));
    expect(asked[1]?.edits).toEqual([
      { op: "set_name", activity: "A3", name: "Pour" },
      { op: "change_link", predecessor: "A1", successor: "A2", link_type: "FS", new_type: "SS", lag: "2d" },
    ]);
  });

  it("takes a change back while a preview is out, once it's done", async () => {
    let release: () => void = () => {};
    const asked: Record<string, unknown>[] = [];
    const api = new Api("token", (_input, init) => {
      asked.push(JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>);
      if (asked.length === 1) return new Promise((done) => (release = () => done(json(REPORT))));
      return Promise.resolve(json(REPORT));
    });
    const editor = new Editing(api, null);
    editor.show({ rows: sample(), head: "h1", writable: true });
    const rows = sample();
    const removal = removeLink(rows, LINK);
    const out = editor.change([removal, addLink(rows, 2, 3, "FS", null)]);
    // Kept back by the user while the preview runs: no longer shown at once.
    await editor.drop(removal.key);
    expect(changesWaiting(editor.state()).map((c) => c.edit.op)).toEqual(["add_link"]);
    release();
    await out;
    await new Promise((done) => setTimeout(done, 0));
    expect(asked[1]?.edits).toEqual([{ op: "add_link", predecessor: "A2", successor: "A3", link_type: "FS" }]);
    expect(editor.state().pending?.changes?.map((c) => c.edit.op)).toEqual(["add_link"]);
  });

  it("marks a refused link edit by its key, and says a change was refused, with the engine's words", async () => {
    const { editor } = editing(() =>
      json(
        {
          schema: "totalfloat.error/1",
          error: {
            code: "edits-refused",
            message: "1 of 1 edits were refused, so nothing was changed",
            hint: "fix the edits",
            problems: [{ edit: 1, code: "circular-logic", message: "A2 to A1 would close a loop: A1 to A2 back to A1", hint: "remove a link in that loop, or link the other way" }],
          },
        },
        422,
      ),
    );
    const loop = addLink(sample(), 2, 1, "FS", null);
    await editor.change([loop]);
    expect(editor.state().marks.get(loop.key as CellKey)?.message).toBe("A2 to A1 would close a loop: A1 to A2 back to A1");
    expect(editor.state().notice?.text).toBe("A change was refused: A2 to A1 would close a loop: A1 to A2 back to A1");
    // Kept, to fix or drop.
    expect(editor.state().pending?.changes).toHaveLength(1);
  });
});
