// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useSyncExternalStore } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api } from "../api";
import { View } from "../view";
import { ChangesPanel } from "./ChangesPanel";
import { Editing } from "./editing";
import { sample } from "./fixture";

afterEach(cleanup);

const json = (body: unknown, status = 200, head?: string) =>
  new Response(JSON.stringify(body), { status, headers: head ? { "x-totalfloat-head": head } : {} });

/** An edit report for project 7 moving its finish to `finish`, with `extra` on the project. */
function report(finish: string, extra: Record<string, unknown> = {}, recorded?: string) {
  return {
    schema: "totalfloat.edit/1",
    file: "plant.tfdb@main",
    output: null,
    applied: ["A1 duration", "A2 duration"],
    warnings: [],
    projects: [
      {
        id: 7,
        code: "P",
        data_date: "2026-01-05 08:00",
        scheduled_finish: finish,
        previous_finish: "2026-01-09 17:00",
        activity_finish: finish,
        activities: 3,
        critical: 3,
        previous_critical: 3,
        changed_activities: 1,
        changed_dates_or_float: 1,
        changes: {
          by_field: { early_finish: 1 },
          starts_at: ["A2"],
          starts_at_total: 1,
          activities: [{ code: "A2", name: "Name A2", fields: [{ field: "early_finish", from: "2026-01-09 17:00", to: finish }] }],
        },
        added: [],
        deleted: [],
        milestones_moved_total: 0,
        milestones_moved: [],
        became_critical_total: 0,
        became_critical: [],
        left_critical_total: 0,
        left_critical: [],
        findings_total: 0,
        findings: [],
        checks: { newly_failing: [], more_offenders: [], newly_passing: [] },
        ...extra,
      },
    ],
    ...(recorded ? { recorded: { id: `${recorded}${"0".repeat(54)}`, short_id: recorded, branch: "main" } } : {}),
  };
}

/** A server answering each request with the next of `answers`, keeping what was asked. */
function server(...answers: (() => Response)[]) {
  const asked: Record<string, unknown>[] = [];
  const fetcher: typeof fetch = (_input, init) => {
    asked.push(JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>);
    const answer = answers.shift();
    if (!answer) throw new Error("nothing to answer");
    return Promise.resolve(answer());
  };
  return { api: new Api("token", fetcher), asked };
}

function editingOn(api: Api) {
  const storage = new Map<string, string>();
  const store = {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => void storage.set(k, v),
  } as unknown as Storage;
  const editor = new Editing(api, store);
  editor.show({ rows: sample(), head: "h1", writable: true });
  editor.reloadWith(() => Promise.resolve({ rows: sample(), head: "h2", writable: true }));
  return editor;
}

function Shown({ editing, view }: { editing: Editing; view: View }) {
  const state = useSyncExternalStore(editing.subscribe, editing.state);
  return <ChangesPanel editing={editing} state={state} rows={sample()} view={view} mac={false} onClose={() => {}} />;
}

function panel(editing: Editing) {
  const view = new View();
  render(<Shown editing={editing} view={view} />);
  return { view, region: () => screen.getByRole("region", { name: "Changes" }) };
}

/** Two original durations typed, previewed together. */
async function twoEdits(editing: Editing) {
  await act(async () => {
    await editing.commit([{ id: 1, column: "original", text: "6d" }]);
    await editing.commit([{ id: 2, column: "original", text: "7d" }]);
  });
}

describe("the Changes panel", () => {
  it("says nothing waits until an edit is typed", () => {
    const { region } = panel(editingOn(server().api));
    expect(region().textContent).toContain("No edits are waiting");
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
  });

  it("lists each edit waiting with its old and new value, and what they do together", async () => {
    const { api, asked } = server(
      () => json(report("2026-01-12 17:00")),
      () => json(report("2026-01-14 17:00")),
    );
    const editing = editingOn(api);
    const { region, view } = panel(editing);
    await twoEdits(editing);
    expect(asked[1]).toMatchObject({ dry_run: true, check: true });
    const edits = within(screen.getByRole("list", { name: "Edits waiting" })).getAllByRole("listitem");
    expect(edits).toHaveLength(2);
    expect(edits[0]?.textContent).toContain("A1");
    expect(edits[0]?.textContent).toContain("Original duration: 40h → 6d");
    expect(edits[1]?.textContent).toContain("Original duration: 40h → 7d");
    expect(region().textContent).toContain("2 edits waiting");
    expect(region().textContent).toContain("Finish 09 Jan 26 → 14 Jan 26");
    // A moved activity selects itself in the grid.
    const reveal = vi.spyOn(view, "reveal");
    act(() => within(screen.getByRole("list", { name: "Moved activities" })).getByRole("button").click());
    expect(reveal).toHaveBeenCalledWith(2);
  });

  it("drops one edit and previews the other alone, and clears when the last goes", async () => {
    const { api, asked } = server(
      () => json(report("2026-01-12 17:00")),
      () => json(report("2026-01-14 17:00")),
      () => json(report("2026-01-13 17:00")),
    );
    const editing = editingOn(api);
    const { region } = panel(editing);
    await twoEdits(editing);
    await act(async () => {
      screen.getByRole("button", { name: "Drop A1 Original duration" }).click();
      await Promise.resolve();
    });
    await act(async () => {
      for (let i = 0; i < 5; i++) await Promise.resolve();
    });
    expect(asked[2]).toMatchObject({ dry_run: true, edits: [{ op: "set_duration", activity: "A2", original: "7d" }] });
    expect(within(screen.getByRole("list", { name: "Edits waiting" })).getAllByRole("listitem")).toHaveLength(1);
    expect(region().textContent).toContain("Finish 09 Jan 26 → 13 Jan 26");
    await act(async () => {
      screen.getByRole("button", { name: "Drop A2 Original duration" }).click();
      await Promise.resolve();
    });
    expect(asked).toHaveLength(3);
    expect(region().textContent).toContain("No edits are waiting");
    expect(editing.state().values.size).toBe(0);
  });

  it("applies with the message as one changeset, and discards", async () => {
    const { api, asked } = server(
      () => json(report("2026-01-12 17:00")),
      () => json(report("2026-01-14 17:00")),
      () => json(report("2026-01-14 17:00", {}, "abc1234567"), 200, "h2"),
      () => json(report("2026-01-12 17:00")),
    );
    const editing = editingOn(api);
    const { region } = panel(editing);
    await twoEdits(editing);
    fireEvent.change(screen.getByRole("textbox", { name: "Message for the change (optional)" }), { target: { value: "Longer cure" } });
    await act(async () => {
      screen.getByRole("button", { name: "Apply" }).click();
      for (let i = 0; i < 10; i++) await Promise.resolve();
    });
    expect(asked[2]).toMatchObject({ message: "Longer cure", head: "h1" });
    expect(asked[2]?.edits).toHaveLength(2);
    expect(asked[2]?.dry_run).toBeUndefined();
    // Recording doesn't refuse for a check: the preview showed them.
    expect(asked[2]?.check).toBeUndefined();
    expect(region().textContent).toContain("Recorded abc1234567");
    expect(region().textContent).toContain("No edits are waiting");

    await act(async () => {
      await editing.commit([{ id: 3, column: "name", text: "Cure" }]);
    });
    // Escape from anywhere in the panel discards, as from a moved activity just clicked.
    fireEvent.keyDown(screen.getByRole("button", { name: "Drop A3 Activity name" }), { key: "Escape" });
    expect(editing.state().pending).toBeNull();
    expect(region().textContent).toContain("Discarded.");
  });

  it("lists a quality check the edits make newly fail, read from the server's 422", async () => {
    const failing = {
      checks: {
        newly_failing: [{ id: "high-float", name: "High float", measure: "1 of 3 (33%)", added: ["A2"] }],
        more_offenders: [{ id: "open-ends", name: "Open ends", measure: "2 of 3", added: ["A3"] }],
        newly_passing: ["Negative float"],
      },
    };
    const { api } = server(() => json(report("2026-01-12 17:00", failing), 422));
    const editing = editingOn(api);
    const { region } = panel(editing);
    await act(async () => {
      await editing.commit([{ id: 2, column: "original", text: "1d" }]);
    });
    const checks = screen.getByRole("list", { name: "Quality checks newly failing" });
    expect(checks.textContent).toContain("High float");
    expect(checks.textContent).toContain("A2");
    // Failing before too, so not newly.
    expect(checks.textContent).not.toContain("Open ends");
    expect(screen.getByRole("list", { name: "Quality checks newly passing" }).textContent).toContain("Negative float");
    expect(editing.state().notice?.text).toContain("1 check newly fails");
    expect(screen.getByRole("button", { name: "Apply" })).toBeTruthy();
    expect(region().textContent).not.toContain("Not previewed");
  });

  it("shows a refusal beside the edit it came from, with its hint", async () => {
    const { api } = server(() =>
      json(
        {
          schema: "totalfloat.error/1",
          error: {
            code: "edits-refused",
            message: "1 edit was refused",
            hint: "",
            problems: [{ edit: 2, source: "edit 2", code: "bad-span", message: "a duration can't be negative", hint: "type 0 or more" }],
          },
        },
        422,
      ),
    );
    const editing = editingOn(api);
    panel(editing);
    await act(async () => {
      await editing.commit([
        { id: 1, column: "name", text: "Fine" },
        { id: 2, column: "original", text: "-5d" },
      ]);
    });
    const edits = within(screen.getByRole("list", { name: "Edits waiting" })).getAllByRole("listitem");
    expect(edits[0]?.textContent).not.toContain("negative");
    expect(edits[1]?.textContent).toContain("a duration can't be negative · type 0 or more");
    expect(edits[1]?.className).toBe("refused");
  });
});
