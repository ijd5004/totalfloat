import { describe, expect, it } from "vitest";

import { Api } from "../api";
import { showUnits } from "../grid/columns";
import { cellKey } from "./cells";
import { type Change, Editing, type Shown, changesWaiting, waiting, withDependents } from "./editing";
import { sample } from "./fixture";

const json = (body: unknown, status = 200, head?: string) =>
  new Response(JSON.stringify(body), { status, headers: head ? { "x-totalfloat-head": head } : {} });

const refusal = (code: string, message: string, problems: unknown[] = []) =>
  json({ schema: "totalfloat.error/1", error: { code, message, hint: `hint for ${code}`, problems } }, code === "branch-moved" ? 409 : 422);

function report(recorded?: string) {
  return {
    schema: "totalfloat.edit/1",
    file: "plant.tfdb@main",
    output: null,
    applied: ["A1: original duration 40h -> 5d"],
    warnings: [],
    projects: [
      {
        id: 7,
        code: "P",
        scheduled_finish: "2026-01-16 17:00",
        previous_finish: "2026-01-09 17:00",
        changed_dates_or_float: 2,
        became_critical_total: 0,
        left_critical_total: 0,
      },
    ],
    ...(recorded ? { recorded: { id: `${recorded}${"0".repeat(54)}`, short_id: recorded, branch: "main" } } : {}),
  };
}

/** A server that answers each request with the next of `answers`, and keeps what was asked. */
function server(...answers: ((body: Record<string, unknown>) => Response)[]) {
  const asked: { path: string; body: Record<string, unknown> }[] = [];
  const fetcher: typeof fetch = (input, init) => {
    const body = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>;
    asked.push({ path: String(input), body });
    const answer = answers.shift();
    if (!answer) throw new Error(`nothing to answer ${String(input)}`);
    return Promise.resolve(answer(body));
  };
  return { api: new Api("token", fetcher), asked };
}

/** Editing on a page showing `rows` at head `h1`; each reload moves the head on. */
function editing(api: Api, previewing = true) {
  const storage = new Map<string, string>();
  const store = {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => void storage.set(k, v),
  } as unknown as Storage;
  if (!previewing) store.setItem("totalfloat.preview-edits", "off");
  const editor = new Editing(api, store);
  let shown: Shown = { rows: sample(), head: "h1", writable: true };
  let reloads = 0;
  editor.show(shown);
  editor.reloadWith(() => {
    reloads++;
    shown = { ...shown, head: `h${reloads + 1}` };
    return Promise.resolve(shown);
  });
  return { editor, reloads: () => reloads };
}

describe("editing", () => {
  it("previews a commit, then records it on apply with the message", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report("abc1234567"), 200, "h2"),
    );
    const { editor, reloads } = editing(api);
    await editor.commit([{ id: 1, column: "original", text: "5d" }]);
    expect(asked[0]?.body).toMatchObject({ dry_run: true, head: "h1", project: 7, edits: [{ op: "set_duration", activity: "A1", original: "5d" }] });
    expect(editor.state().notice?.text).toBe("Preview: finish 09 Jan 26 → 16 Jan 26, 2 activities changed.");
    expect(editor.state().values.get(cellKey(1, "original"))).toBe("5d");

    editor.setMessage("Longer cure");
    await editor.apply();
    expect(asked[1]?.body).toMatchObject({ head: "h1", message: "Longer cure" });
    expect(asked[1]?.body.dry_run).toBeUndefined();
    expect(reloads()).toBe(1);
    expect(editor.state()).toMatchObject({ pending: null, message: "", busy: false });
    expect(editor.state().values.size).toBe(0);
    expect(editor.state().made).toEqual([{ id: `abc1234567${"0".repeat(54)}`, short: "abc1234567", what: "A1: original duration 40h -> 5d" }]);
    expect(editor.state().notice?.text).toMatch(/^Recorded abc1234567: finish/);
  });

  it("records at once with previews off", async () => {
    const { api, asked } = server(() => json(report("abc1234567")));
    const { editor } = editing(api, false);
    await editor.commit([{ id: 2, column: "name", text: "Pour" }]);
    expect(asked).toHaveLength(1);
    expect(asked[0]?.body.dry_run).toBeUndefined();
    expect(editor.state().made).toHaveLength(1);
  });

  it("marks the cells a refusal names, by each problem's place in the batch", async () => {
    const { api } = server(() =>
      refusal("edits-refused", "2 edits were refused", [
        { edit: 2, source: "edit 2", code: "bad-span", message: "a duration can't be negative", hint: "type 0 or more" },
        { source: "options", code: "x", message: "loose", hint: "" },
      ]),
    );
    const { editor } = editing(api);
    await editor.commit([
      { id: 1, column: "name", text: "Fine" },
      { id: 2, column: "original", text: "-5d" },
    ]);
    const state = editor.state();
    expect([...state.marks.keys()]).toEqual([cellKey(2, "original")]);
    expect(state.marks.get(cellKey(2, "original"))?.message).toBe("a duration can't be negative");
    expect(state.notice).toMatchObject({ tone: "bad" });
    // Still typed, so fixing the one cell sends both again.
    expect(state.values.size).toBe(2);
  });

  it("marks a value it can't send without asking the server, and sends the rest once it's fixed", async () => {
    const { api, asked } = server(() => json(report()));
    const { editor } = editing(api);
    await editor.commit([
      { id: 1, column: "name", text: "Fine" },
      { id: 2, column: "original", text: "5x" },
    ]);
    expect(asked).toHaveLength(0);
    expect(editor.state().marks.has(cellKey(2, "original"))).toBe(true);
    expect(editor.state().notice?.text).toBe(`A cell can't be sent: "5x" isn't a duration`);
    await editor.commit([{ id: 2, column: "original", text: "5d" }]);
    expect(asked[0]?.body.edits).toEqual([
      { op: "set_name", activity: "A1", name: "Fine" },
      { op: "set_duration", activity: "A2", original: "5d" },
    ]);
    expect(editor.state().marks.size).toBe(0);
  });

  it("keeps what was typed when another writer moved the branch, and previews it again on apply", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => refusal("branch-moved", "main has moved on since the page saw it"),
      () => json(report()),
    );
    const { editor, reloads } = editing(api);
    await editor.commit([{ id: 1, column: "original", text: "5d" }]);
    await editor.apply();
    expect(reloads()).toBe(1);
    expect(editor.state().banner).toContain("your typed value is kept");
    expect(editor.state().values.get(cellKey(1, "original"))).toBe("5d");
    expect(editor.state().pending?.preview).toBeNull();
    await editor.apply();
    // Previewed again, at the head shown now.
    expect(asked[2]?.body).toMatchObject({ dry_run: true, head: "h2" });
    expect(editor.state().banner).toBeNull();
  });

  it("undoes the last changeset this page made, saying which", async () => {
    const { api, asked } = server(
      () => json(report("abc1234567")),
      () =>
        json({
          schema: "totalfloat.history-revert/1",
          reverted: `abc1234567${"0".repeat(54)}`,
          recorded: { id: "f".repeat(64), short_id: "ffffffffff", branch: "main" },
          replayed: [],
          impact: null,
        }),
    );
    const { editor } = editing(api, false);
    await editor.undo();
    expect(editor.state().notice?.text).toBe("Nothing this page recorded is left to undo.");
    await editor.commit([{ id: 1, column: "name", text: "Pour" }]);
    await editor.undo();
    expect(asked[1]).toMatchObject({ path: "/api/1/history/revert", body: { at: `abc1234567${"0".repeat(54)}`, head: "h2" } });
    expect(editor.state().made).toHaveLength(0);
    expect(editor.state().notice?.text).toBe("Took out abc1234567 (A1: original duration 40h -> 5d) as ffffffffff.");
  });

  it("sends cells committed while a request is out once it's done, with what's pending", async () => {
    let answer: (response: Response) => void = () => {};
    const asked: Record<string, unknown>[] = [];
    const api = new Api("token", (_input, init) => {
      asked.push(JSON.parse(String(init?.body)) as Record<string, unknown>);
      return asked.length === 1 ? new Promise((done) => (answer = done)) : Promise.resolve(json(report()));
    });
    const { editor } = editing(api);
    const first = editor.commit([{ id: 1, column: "original", text: "5d" }]);
    expect(editor.state().busy).toBe(true);
    await editor.commit([{ id: 2, column: "original", text: "6d" }]);
    // Shown at once, sent later.
    expect(editor.state().values.get(cellKey(2, "original"))).toBe("6d");
    expect(asked).toHaveLength(1);
    answer(json(report()));
    await first;
    await new Promise((done) => setTimeout(done, 0));
    expect(asked).toHaveLength(2);
    expect(asked[1]?.edits).toEqual([
      { op: "set_duration", activity: "A1", original: "5d" },
      { op: "set_duration", activity: "A2", original: "6d" },
    ]);
  });

  it("takes the head a change answered with when the page's reload is overtaken", async () => {
    const { api, asked } = server(
      () => json(report("abc1234567"), 200, "h9"),
      () => json(report()),
    );
    const { editor } = editing(api, false);
    editor.reloadWith(() => Promise.resolve(null));
    await editor.commit([{ id: 1, column: "name", text: "Pour" }]);
    editor.setPreviewing(true);
    await editor.commit([{ id: 2, column: "name", text: "Cure" }]);
    expect(asked[1]?.body.head).toBe("h9");
  });

  it("previews what's typed again after an undo moves the branch under it", async () => {
    const { api, asked } = server(
      () => json(report("abc1234567")),
      () => json(report()),
      () => json({ schema: "totalfloat.history-revert/1", reverted: "a", recorded: null, replayed: [], impact: null }),
      () => json(report()),
    );
    const { editor } = editing(api, false);
    await editor.commit([{ id: 1, column: "name", text: "Pour" }]);
    editor.setPreviewing(true);
    await editor.commit([{ id: 2, column: "name", text: "Cure" }]);
    expect(editor.state().pending?.preview).not.toBeNull();
    await editor.undo();
    expect(editor.state().pending?.preview).toBeNull();
    expect(editor.state().notice?.hint).toContain("typed values are kept");
    await editor.apply();
    expect(asked[3]?.body).toMatchObject({ dry_run: true, head: "h3" });
  });

  it("doesn't keep pasted cells that changed nothing, so they can't undo another writer's change", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    // A row pasted back: the name as it was, and a new duration.
    await editor.commit([
      { id: 1, column: "name", text: "Name A1" },
      { id: 1, column: "original", text: "5d" },
    ]);
    // Kept with the unit a bare number would have meant as it was typed.
    expect(editor.state().pending?.typed).toEqual([{ id: 1, column: "original", text: "5d", unit: "h" }]);
    // Someone renames A1; the page shows it; another cell joins the preview.
    editor.show({ rows: sample({ name: ["Renamed", "Name A2", "Name A3"] }), head: "h5", writable: true });
    await editor.commit([{ id: 2, column: "name", text: "Cure" }]);
    expect(asked[1]?.body.edits).toEqual([
      { op: "set_duration", activity: "A1", original: "5d" },
      { op: "set_name", activity: "A2", name: "Cure" },
    ]);
  });

  it("keeps a previewed duration in the unit it was typed in when the units shown change", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    editor.show({ rows: sample({ hours_per_day: [8, 8, 8] }), head: "h1", writable: true });
    try {
      await editor.commit([{ id: 1, column: "original", text: "6" }]);
      showUnits("hours");
      await editor.commit([{ id: 2, column: "name", text: "Cure" }]);
    } finally {
      showUnits("days");
    }
    expect(asked[1]?.body.edits).toEqual([
      { op: "set_duration", activity: "A1", original: "6d" },
      { op: "set_name", activity: "A2", name: "Cure" },
    ]);
    // It shows, and so copies, with that unit.
    expect(editor.state().values.get(cellKey(1, "original"))).toBe("6d");
  });

  it("keeps a remaining held with a new original as its stored hours, so copying it back can't round it", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    // Not started: 36.5 hours remaining shows as 4.56 days on an 8-hour day.
    const rows = sample({ hours_per_day: [8, 8, 8], original_duration_hours: [36.5, 40, 40], remaining_duration_hours: [36.5, 40, 40] });
    editor.show({ rows, head: "h1", writable: true });
    await editor.commit([
      { id: 1, column: "original", text: "6" },
      { id: 1, column: "remaining", text: "4.56" },
    ]);
    const held = editor.state().values.get(cellKey(1, "remaining"));
    expect(held).toBe("36.5h");
    // The pending cell copied and pasted back sends the same hours.
    await editor.commit([{ id: 1, column: "remaining", text: held ?? "" }]);
    expect(asked[1]?.body.edits).toEqual([{ op: "set_duration", activity: "A1", original: "6d", remaining: "36.5h" }]);
  });

  it("refuses to edit a schedule that can't change", async () => {
    const { api, asked } = server();
    const { editor } = editing(api);
    editor.show({ rows: sample(), head: null, writable: false });
    await editor.commit([{ id: 1, column: "name", text: "X" }]);
    expect(asked).toHaveLength(0);
    expect(editor.state().notice?.text).toBe("This schedule is read-only here.");
  });

  it("asks the preview for the quality checks, and reads a check that newly fails as the preview", async () => {
    const blocked = report();
    const [project] = blocked.projects;
    Object.assign(project ?? {}, {
      checks: { newly_failing: [{ id: "high-float", name: "High float", measure: "1 of 3", added: ["A1"] }], more_offenders: [], newly_passing: [] },
    });
    const { api, asked } = server(
      () => json(blocked, 422),
      () => json(report("abc1234567")),
    );
    const { editor } = editing(api);
    await editor.commit([{ id: 1, column: "original", text: "5d" }]);
    expect(asked[0]?.body).toMatchObject({ dry_run: true, check: true });
    expect(editor.state().pending?.preview).not.toBeNull();
    expect(editor.state().notice?.text).toContain("1 check newly fails");
    // Recording takes it as previewed: the checks aren't asked again.
    await editor.apply();
    expect(asked[1]?.body.check).toBeUndefined();
    expect(editor.state().made).toHaveLength(1);
  });

  it("drops one edit and previews the rest, even with previews off, and clears when it was the last", async () => {
    const { api, asked } = server(
      () => refusal("edits-refused", "refused", [{ edit: 1, source: "edit 1", code: "x", message: "no", hint: "" }]),
      () => json(report()),
    );
    const { editor } = editing(api, false);
    await editor.commit([
      { id: 1, column: "original", text: "5d" },
      { id: 2, column: "name", text: "Cure" },
    ]);
    expect(editor.state().marks.has(cellKey(1, "original"))).toBe(true);
    await editor.drop(cellKey(1, "original"));
    expect(asked[1]?.body).toMatchObject({ dry_run: true, edits: [{ op: "set_name", activity: "A2", name: "Cure" }] });
    expect(editor.state().marks.size).toBe(0);
    expect([...editor.state().values.keys()]).toEqual([cellKey(2, "name")]);
    expect(editor.state().pending?.preview).not.toBeNull();
    await editor.drop(cellKey(2, "name"));
    expect(asked).toHaveLength(2);
    expect(editor.state()).toMatchObject({ pending: null, busy: false });
    expect(editor.state().values.size).toBe(0);
  });

  it("forgets a message typed for edits that were discarded, so the next edit applied records none", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
      () => json(report("abc1234567"), 200, "h2"),
    );
    const { editor } = editing(api);
    await editor.commit([{ id: 2, column: "original", text: "5d" }]);
    editor.setMessage("keepme");
    editor.discard();
    expect(editor.state()).toMatchObject({ pending: null, message: "" });
    await editor.commit([{ id: 2, column: "original", text: "6d" }]);
    expect(editor.state().message).toBe("");
    await editor.apply();
    expect(asked).toHaveLength(3);
    expect(asked[2]?.body.dry_run).toBeUndefined();
    expect(asked[2]?.body).not.toHaveProperty("message");
    expect(editor.state().made).toHaveLength(1);
  });

  it("keeps the message while some edits wait, and forgets it when the last is dropped", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
      () => json(report()),
      () => json(report("abc1234567"), 200, "h2"),
    );
    const { editor } = editing(api);
    await editor.commit([
      { id: 1, column: "original", text: "5d" },
      { id: 2, column: "name", text: "Cure" },
    ]);
    editor.setMessage("Longer cure");
    await editor.drop(cellKey(1, "original"));
    expect(editor.state().message).toBe("Longer cure");
    await editor.drop(cellKey(2, "name"));
    expect(editor.state()).toMatchObject({ pending: null, message: "" });
    await editor.commit([{ id: 2, column: "original", text: "6d" }]);
    await editor.apply();
    expect(asked[3]?.body).not.toHaveProperty("message");
  });

  it("forgets the message when the last edit is dropped while its preview is out", async () => {
    let answer: (response: Response) => void = () => {};
    const asked: Record<string, unknown>[] = [];
    const api = new Api("token", (_input, init) => {
      asked.push(JSON.parse(String(init?.body)) as Record<string, unknown>);
      return new Promise((done) => (answer = done));
    });
    const { editor } = editing(api);
    const add = { key: "change:add:A10", edit: { op: "add_activity" as const, code: "A10", name: "A10", wbs: "P", duration: "1d" }, words: "Add A10" };
    const first = editor.change([add]);
    editor.setMessage("New pour");
    // Taken back while the preview is out: dropped once it's done.
    await editor.drop("change:add:A10");
    expect(editor.state().message).toBe("New pour");
    answer(json(report()));
    await first;
    await new Promise((done) => setTimeout(done, 0));
    expect(asked).toHaveLength(1);
    expect(editor.state()).toMatchObject({ pending: null, message: "", busy: false });
  });

  it("sends changes to the schedule's shape after the typed cells, and drops one by its key", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    await editor.commit([{ id: 1, column: "name", text: "Cure" }]);
    const adds = { code: "A10", name: "Survey", wbs: "P", duration: "2" };
    await editor.change([
      { key: "change:add:A10", edit: { op: "add_activity", code: "A10", name: "Survey", wbs: "P", duration: "2d" }, words: "Add A10", adds },
      { key: "change:delete:A2", edit: { op: "delete_activity", activity: "A2" }, words: "Delete A2" },
    ]);
    expect(asked[1]?.body).toMatchObject({
      dry_run: true,
      edits: [
        { op: "set_name", activity: "A1", name: "Cure" },
        { op: "add_activity", code: "A10" },
        { op: "delete_activity", activity: "A2" },
      ],
    });
    expect(waiting(editor.state())).toBe(3);
    await editor.drop("change:add:A10");
    expect(asked[2]?.body).toMatchObject({
      edits: [
        { op: "set_name", activity: "A1", name: "Cure" },
        { op: "delete_activity", activity: "A2" },
      ],
    });
    expect(editor.state().pending?.changes?.map((change) => change.key)).toEqual(["change:delete:A2"]);
    expect(waiting(editor.state())).toBe(2);
  });

  it("shows changes taken while a request is out, and drops several at once", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    const add = (code: string) => ({
      key: `change:add:${code}`,
      edit: { op: "add_activity" as const, code, name: code, wbs: "P", duration: "1d" },
      words: `Add ${code}`,
      adds: { code, name: code, wbs: "P", duration: "1" },
    });
    const first = editor.change([add("A10")]);
    // The first preview is out: the next add waits, shown and counted.
    expect(editor.state().busy).toBe(true);
    void editor.change([add("A20")]);
    expect(changesWaiting(editor.state()).map((c) => c.key)).toEqual(["change:add:A10", "change:add:A20"]);
    expect(waiting(editor.state())).toBe(2);
    await first;
    await new Promise((done) => setTimeout(done, 0));
    expect(editor.state().queued).toEqual([]);
    expect(asked[1]?.body).toMatchObject({ edits: [{ code: "A10" }, { code: "A20" }] });
    await editor.drop("change:add:A10", "change:add:A20");
    expect(editor.state().pending).toBeNull();
    expect(asked).toHaveLength(2);
  });

  it("sends nothing while the edits waiting are in more than one project, and says so", async () => {
    const { api, asked } = server();
    const { editor } = editing(api);
    await editor.change([
      { key: "change:delete:A1", edit: { op: "delete_activity", activity: "A1" }, words: "Delete A1", project: 7 },
      { key: "change:delete:A9", edit: { op: "delete_activity", activity: "A9" }, words: "Delete A9", project: 8 },
    ]);
    expect(asked).toHaveLength(0);
    expect(editor.state().notice?.text).toBe("These edits are in more than one project, and one request takes one.");
    expect(waiting(editor.state())).toBe(2);
  });

  it("sends nothing while typed cells in two projects wait with a change in one", async () => {
    const { api, asked } = server(() => json(report()));
    const { editor } = editing(api);
    editor.show({ rows: sample({ project: [7, 8, 8] }), head: "h1", writable: true });
    await editor.commit([
      { id: 1, column: "name", text: "Cure" },
      { id: 2, column: "name", text: "Set" },
    ]);
    expect(asked[0]?.body).not.toHaveProperty("project");
    await editor.change([{ key: "change:delete:A3", edit: { op: "delete_activity", activity: "A3" }, words: "Delete A3", project: 8 }]);
    expect(asked).toHaveLength(1);
    expect(editor.state().notice?.text).toBe("These edits are in more than one project, and one request takes one.");
  });

  it("names the project of the changes in the request", async () => {
    const { api, asked } = server(() => json(report()));
    const { editor } = editing(api);
    await editor.change([{ key: "change:delete:A1", edit: { op: "delete_activity", activity: "A1" }, words: "Delete A1", project: 7 }]);
    expect(asked[0]?.body).toMatchObject({ project: 7, edits: [{ op: "delete_activity", activity: "A1" }] });
  });

  it("replaces a change waiting with one made again under its key, as a move to another node", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    const move = (wbs: string) => ({ key: "change:move:A1", edit: { op: "set_wbs" as const, activity: "A1", wbs }, words: `Move A1 to ${wbs}`, moves: wbs });
    await editor.change([move("2")]);
    await editor.change([move("3")]);
    expect(asked[1]?.body).toMatchObject({ edits: [{ op: "set_wbs", activity: "A1", wbs: "3" }] });
    expect(editor.state().pending?.changes).toHaveLength(1);
  });

  it("keeps a rename made again where the first was, ahead of the edits that use its new code", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    const rename = (name: string) => ({
      key: "change:rename-wbs:P.1",
      edit: { op: "rename_wbs" as const, wbs: "1", code: "X", name },
      words: `Rename to ${name}`,
      keepPlace: true,
    });
    await editor.change([rename("Civil")]);
    await editor.change([{ key: "change:move:A3", edit: { op: "set_wbs", activity: "A3", wbs: "X" }, words: "Move A3", moves: "P.1" }]);
    await editor.change([rename("Civil works")]);
    expect(asked[2]?.body).toMatchObject({
      edits: [
        { op: "rename_wbs", wbs: "1", code: "X", name: "Civil works" },
        { op: "set_wbs", activity: "A3", wbs: "X" },
      ],
    });
  });

  it("drops with a WBS node added the edits after it that name it, and theirs", async () => {
    const { api, asked } = server(
      () => json(report()),
      () => json(report()),
    );
    const { editor } = editing(api);
    const changes: Change[] = [
      { key: "change:add-wbs:P.3", edit: { op: "add_wbs", code: "3", name: "Survey", parent: "P" }, words: "", wbs: { path: "P.3", code: "3" } },
      { key: "change:add:A10", edit: { op: "add_activity", code: "A10", name: "Walk", wbs: "3", duration: "1d" }, words: "" },
      { key: "change:add-wbs:P.3.1", edit: { op: "add_wbs", code: "3a", name: "Under", parent: "3" }, words: "", wbs: { path: "P.3.3a", code: "3a" } },
      { key: "change:move:A2", edit: { op: "set_wbs", activity: "A2", wbs: "3A" }, words: "", moves: "P.3.3a" },
      { key: "change:delete:A1", edit: { op: "delete_activity", activity: "A1" }, words: "" },
    ];
    expect(withDependents(["change:add-wbs:P.3"], changes)).toEqual([
      "change:add-wbs:P.3",
      "change:add:A10",
      "change:add-wbs:P.3.1",
      "change:move:A2",
    ]);
    await editor.change(changes);
    await editor.drop("change:add-wbs:P.3");
    expect(editor.state().pending?.changes?.map((c) => c.key)).toEqual(["change:delete:A1"]);
    expect(asked[1]?.body).toMatchObject({ edits: [{ op: "delete_activity", activity: "A1" }] });
  });

  it("remembers whether to preview", () => {
    const { api } = server();
    const { editor } = editing(api);
    expect(editor.state().previewing).toBe(true);
    editor.setPreviewing(false);
    expect(editor.state().previewing).toBe(false);
  });
});
