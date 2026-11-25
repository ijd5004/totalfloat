// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { Api, type Links } from "../api";
import { sample } from "../edit/fixture";
import { DEFAULT_ARRANGEMENT, arrange, buildModel } from "../grid/model";
import { View } from "../view";
import { Inspector, type InspectorEditing } from "./Inspector";
import type { LinkChange } from "./linkEdits";
import type { LinkEditing } from "./Logic";

afterEach(cleanup);

/** A1 → A2, FS, no lag. Four activities: A1 to A3 and Glazing L4. */
const LINKS: Links = {
  schema: "totalfloat.links/1",
  count: 1,
  id: [11],
  predecessor: [1],
  successor: [2],
  type: ["FS"],
  lag_hours: [0],
  driving: [true],
};

function rowsOf() {
  const rows = sample({
    id: [1, 2, 3, 4],
    code: ["A1", "A2", "A3", "A4"],
    name: ["Glazing L1", "Glazing L2", "Glazing L3", "Sealants"],
    project: [7, 7, 7, 7],
    calendar: ["Five day", "Five day", "Five day", "Five day"],
    hours_per_day: [8, 8, 8, 8],
    early_finish: ["2026-01-09 17:00", "2026-01-16 17:00", "2026-01-23 17:00", "2026-01-30 17:00"],
  });
  (rows as { count: number }).count = 4;
  for (const key of Object.keys(rows) as (keyof typeof rows)[]) {
    const value = rows[key];
    if (Array.isArray(value) && value.length === 3) (rows as unknown as Record<string, unknown[]>)[key] = [...value, value[2]];
  }
  return rows;
}

/** Explain and query answers aren't what these tests look at. */
const api = new Api("token", () =>
  Promise.resolve(new Response(JSON.stringify({ schema: "totalfloat.query/1", file: "x", total: 0, offset: 0, limit: 0, returned: 0, columns: [], rows: [], chains: [] }), { status: 200 })),
);

/** The inspector on activity `line`, with link edits kept as the pending-edits flow keeps them: one per key. */
function Harness({ view, sent }: { view: View; sent: LinkChange[][] }) {
  const [waiting, setWaiting] = useState<LinkChange[]>([]);
  const links: LinkEditing = {
    enabled: true,
    waiting,
    marks: new Map(),
    change: (change) =>
      setWaiting((now) => {
        const next = [...now.filter((c) => c.key !== change.key), change];
        sent.push(next);
        return next;
      }),
    drop: (key) => setWaiting((now) => now.filter((c) => c.key !== key)),
  };
  const edit: InspectorEditing = { enabled: true, values: new Map(), marks: new Map(), commit: () => {}, links };
  return <Inspector api={api} view={view} links={LINKS} fields={null} edit={edit} onClose={() => {}} />;
}

function on(code: string) {
  const view = new View();
  const rows = rowsOf();
  const arranged = arrange(buildModel(rows), DEFAULT_ARRANGEMENT);
  const line = [...arranged.lines].findIndex((i) => i >= 0 && rows.code[i] === code);
  view.publish({ rows, arranged, top: 0, height: 400, rowHeight: 22, active: line, bodyTop: 0 });
  const sent: LinkChange[][] = [];
  render(<Harness view={view} sent={sent} />);
  return { sent, last: () => sent.at(-1) ?? [] };
}

async function settle() {
  await act(async () => {
    await new Promise((done) => setTimeout(done, 150));
  });
}

describe("adding a link in the inspector", () => {
  it("lists matches for part of a name, with their finish, marks those already linked, and adds FS on Enter", async () => {
    const { last } = on("A2");
    const field = screen.getByRole("combobox", { name: /^Add predecessor/ });
    fireEvent.change(field, { target: { value: "glaz" } });
    const options = within(screen.getByRole("listbox", { name: "Activities to add as a predecessor" })).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["A1Glazing L1already linked", "A3Glazing L3ends 23 Jan 26"]);
    expect(options[0]?.getAttribute("aria-disabled")).toBe("true");
    // Enter on one already linked is refused; down to A3 and Enter adds it.
    fireEvent.keyDown(field, { key: "Enter" });
    expect(screen.getByRole("alert").textContent).toBe("A1 is already linked.");
    fireEvent.keyDown(field, { key: "ArrowDown" });
    expect(field.getAttribute("aria-activedescendant")).toBe(options[1]?.id);
    fireEvent.keyDown(field, { key: "Enter" });
    expect(last().map((c) => c.edit)).toEqual([{ op: "add_link", predecessor: "A3", successor: "A2", link_type: "FS" }]);
    // It waits in the list, and the field is clear for the next.
    expect(screen.getByRole("button", { name: "A3 Glazing L3, to be added" })).toBeTruthy();
    expect((field as HTMLInputElement).value).toBe("");
    await settle();
  });

  it("takes the type and lag after Tab, and adds a successor", async () => {
    const { last } = on("A2");
    const field = screen.getByRole("combobox", { name: /^Add successor/ });
    fireEvent.change(field, { target: { value: "A4" } });
    fireEvent.keyDown(field, { key: "Tab" });
    expect((field as HTMLInputElement).value).toBe("A4");
    fireEvent.change(screen.getByRole("combobox", { name: "Type of the new successor's link" }), { target: { value: "SS" } });
    const lag = screen.getByRole("textbox", { name: "Lag of the new successor's link" });
    fireEvent.change(lag, { target: { value: "2" } });
    fireEvent.keyDown(lag, { key: "Enter" });
    expect(last().map((c) => c.edit)).toEqual([{ op: "add_link", predecessor: "A2", successor: "A4", link_type: "SS", lag: "2d" }]);
    await settle();
  });

  it("keeps the match highlighted when the pointer moves on to the lag", async () => {
    const { last } = on("A2");
    const field = screen.getByRole("combobox", { name: /^Add successor/ });
    fireEvent.change(field, { target: { value: "A4" } });
    fireEvent.blur(field);
    const lag = screen.getByRole("textbox", { name: "Lag of the new successor's link" });
    fireEvent.change(lag, { target: { value: "1" } });
    fireEvent.keyDown(lag, { key: "Enter" });
    expect(last().map((c) => c.edit)).toEqual([{ op: "add_link", predecessor: "A2", successor: "A4", link_type: "FS", lag: "1d" }]);
    await settle();
  });

  it("shows a link waiting to an activity waiting to be added, whose task_id is negative", () => {
    const view = new View();
    const rows = rowsOf();
    (rows.id as number[])[3] = -1;
    const arranged = arrange(buildModel(rows), DEFAULT_ARRANGEMENT);
    const line = [...arranged.lines].findIndex((i) => i >= 0 && rows.code[i] === "A2");
    view.publish({ rows, arranged, top: 0, height: 400, rowHeight: 22, active: line, bodyTop: 0 });
    const waiting: LinkChange[] = [{ key: "link:add:2>-1", edit: { op: "add_link", predecessor: "A2", successor: "A4", link_type: "FS" }, words: "A2 → A4 added, FS" }];
    const edit: InspectorEditing = {
      enabled: true,
      values: new Map(),
      marks: new Map(),
      commit: () => {},
      links: { enabled: true, waiting, marks: new Map(), change: () => {}, drop: () => {} },
    };
    render(<Inspector api={api} view={view} links={LINKS} fields={null} edit={edit} onClose={() => {}} />);
    expect(screen.getByRole("button", { name: "A4 Sealants, to be added" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Successors 1" })).toBeTruthy();
  });

  it("refuses a lag it can't read, and sends nothing", async () => {
    const { sent } = on("A2");
    const field = screen.getByRole("combobox", { name: /^Add successor/ });
    fireEvent.change(field, { target: { value: "A4" } });
    fireEvent.keyDown(field, { key: "Tab" });
    const lag = screen.getByRole("textbox", { name: "Lag of the new successor's link" });
    fireEvent.change(lag, { target: { value: "soon" } });
    fireEvent.keyDown(lag, { key: "Enter" });
    expect(sent).toEqual([]);
    expect(screen.getByRole("alert").textContent).toContain(`"soon" isn't a lag`);
    await settle();
  });
});

describe("changing and removing a link in the inspector", () => {
  it("makes one change_link of a new type and a new lag", async () => {
    const { last } = on("A2");
    fireEvent.change(screen.getByRole("combobox", { name: "Type of the link with A1" }), { target: { value: "SS" } });
    const lag = screen.getByRole("textbox", { name: "Lag of the link with A1" });
    fireEvent.focus(lag);
    fireEvent.change(lag, { target: { value: "2d" } });
    fireEvent.keyDown(lag, { key: "Enter" });
    expect(last().map((c) => c.edit)).toEqual([{ op: "change_link", predecessor: "A1", successor: "A2", link_type: "FS", new_type: "SS", lag: "2d" }]);
    expect(last()[0]?.words).toBe("A1 → A2: SS, lag 2d");
    // Shown waiting, on the link's own type and lag.
    expect((screen.getByRole("combobox", { name: "Type of the link with A1" }) as HTMLSelectElement).value).toBe("SS");
    expect((screen.getByRole("textbox", { name: "Lag of the link with A1" }) as HTMLInputElement).value).toBe("2d");
    await settle();
  });

  it("puts a lag back on Escape, sending nothing", async () => {
    const { sent } = on("A2");
    const lag = screen.getByRole("textbox", { name: "Lag of the link with A1" }) as HTMLInputElement;
    // Focused for real, so Escape's blur fires at once, as in a browser.
    act(() => lag.focus());
    fireEvent.change(lag, { target: { value: "2d" } });
    fireEvent.keyDown(lag, { key: "Escape" });
    expect(document.activeElement).not.toBe(lag);
    expect(sent).toEqual([]);
    expect(lag.value).toBe("0");
    await settle();
  });

  it("drops the change when the lag goes back to what it was", async () => {
    const { last } = on("A2");
    const lag = screen.getByRole("textbox", { name: "Lag of the link with A1" }) as HTMLInputElement;
    for (const text of ["3d", "0"]) {
      act(() => lag.focus());
      fireEvent.change(lag, { target: { value: text } });
      fireEvent.keyDown(lag, { key: "Enter" });
    }
    expect(last().map((c) => c.edit)).toEqual([{ op: "change_link", predecessor: "A1", successor: "A2", link_type: "FS", lag: "3d" }]);
    expect(document.querySelector(".inspector-link-row.typed")).toBeNull();
    await settle();
  });

  it("drops the change when the type goes back to what it was", async () => {
    const { last } = on("A2");
    const type = screen.getByRole("combobox", { name: "Type of the link with A1" });
    fireEvent.change(type, { target: { value: "FF" } });
    expect(last()).toHaveLength(1);
    fireEvent.change(type, { target: { value: "FS" } });
    expect(document.querySelector(".inspector-link-row.typed")).toBeNull();
    await settle();
  });

  it("removes a link with one remove_link, and keeps it again on Keep", async () => {
    const { last } = on("A1");
    fireEvent.click(screen.getByRole("button", { name: "Remove the link with A2" }));
    expect(last().map((c) => c.edit)).toEqual([{ op: "remove_link", predecessor: "A1", successor: "A2", link_type: "FS" }]);
    expect(screen.getByRole("button", { name: "A2 Glazing L2, FS, driving, to be removed" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep the link with A2" }));
    expect(screen.getByRole("button", { name: "Remove the link with A2" })).toBeTruthy();
    await settle();
  });
});
