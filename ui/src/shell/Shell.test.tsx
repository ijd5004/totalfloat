// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { Api } from "../api";
import { Registry } from "../commands";
import editReport from "../api/golden/edit-1.json";
import proposals from "../api/golden/history-proposals-1.json";
import { golden } from "../api/schemas/golden";
import { inspectOf } from "../api/testing";
import { sample } from "../edit/fixture";
import { Shell, litOf, orderOf } from "./Shell";

/** `/api/1/wbs` with no projects. */
const NO_WBS = { schema: "totalfloat.wbs/1", projects: [] };
const NO_FIELDS = { schema: "totalfloat.fields/1", activity_codes: [], activity_udfs: [] };

afterEach(cleanup);

/** A summary answered later, when the test says, with this head or a failure. */
interface Pending {
  answer: (head: string) => void;
  fail: () => void;
}

/** A server whose summaries wait for the test; everything else answers at once. */
function server() {
  const pending: Pending[] = [];
  const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
  const fetcher: typeof fetch = (input) => {
    const url = String(input);
    if (url.startsWith("/api/1/summary")) {
      return new Promise((resolve) => {
        pending.push({
          answer: (head) =>
            resolve(
              json({
                location: "plant.xer",
                branch: null,
                head,
                writable: false,
                calculated: true,
                inspect: inspectOf("plant.xer", []),
              }),
            ),
          fail: () =>
            resolve(json({ schema: "totalfloat.error/1", error: { code: "store-error", message: "Broken", hint: "" } }, 500)),
        });
      });
    }
    if (url.startsWith("/api/1/rows")) return Promise.resolve(json({ count: 0 }));
    if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
    return Promise.resolve(json({ count: 0 }));
  };
  return { api: new Api(null, fetcher), pending };
}

async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
}

describe("the shell", () => {
  it("shows the newest load when an older one answers last, under StrictMode", async () => {
    const { api, pending } = server();
    render(
      <StrictMode>
        <Shell api={api} registry={new Registry()} follow={false} />
      </StrictMode>,
    );
    await settle();
    expect(pending.length).toBe(2);
    pending[1]?.answer("NEWER-HEAD");
    await settle();
    pending[0]?.answer("OLDER-HEAD");
    await settle();
    expect(screen.getByText("NEWER-HEAD")).toBeTruthy();
    expect(screen.queryByText("OLDER-HEAD")).toBeNull();
  });

  it("ignores an older load's failure once a newer one has shown", async () => {
    const { api, pending } = server();
    render(
      <StrictMode>
        <Shell api={api} registry={new Registry()} follow={false} />
      </StrictMode>,
    );
    await settle();
    pending[1]?.answer("NEWER-HEAD");
    await settle();
    pending[0]?.fail();
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("NEWER-HEAD")).toBeTruthy();
  });

  it("titles the columns in the units chosen in the same render", async () => {
    const { api } = loaded(NO_WBS, sample({ hours_per_day: [8, 8, 8] }));
    const registry = new Registry();
    render(<Shell api={api} registry={registry} follow={false} />);
    await settle();
    expect(screen.getAllByText("Original (d)").length).toBeGreaterThan(0);
    act(() => registry.inOrder().find((action) => action.id === "view.units")?.run());
    expect(screen.queryAllByText("Original (d)")).toHaveLength(0);
    expect(screen.getAllByText("Original (h)").length).toBeGreaterThan(0);
    act(() => registry.inOrder().find((action) => action.id === "view.units")?.run());
  });

  it("shows dates in the format chosen, with or without times, and keeps the choice", async () => {
    localStorage.removeItem("totalfloat.dates");
    // jsdom lays nothing out: a screen's height, so the grid mirrors its rows.
    const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
    try {
      const { api } = loaded();
      const registry = new Registry();
      const run = (id: string) => act(() => registry.inOrder().find((action) => action.id === id)?.run());
      const { unmount } = render(<Shell api={api} registry={registry} follow={false} />);
      await settle();
      const start = () => document.querySelector("#grid-mirror tbody tr td:nth-child(5)")?.textContent;
      expect(start()).toBe("05 Jan 26");
      run("view.dates-yyyy-mm-dd");
      expect(start()).toBe("2026-01-05");
      run("view.date-times");
      expect(start()).toBe("2026-01-05 08:00");
      run("view.dates-mm/dd/yyyy");
      expect(start()).toBe("01/05/2026 08:00");
      await settle();
      unmount();
      expect(JSON.parse(localStorage.getItem("totalfloat.dates") ?? "null")).toEqual({ style: "mm/dd/yyyy", time: true });
    } finally {
      // Back as it was, for the tests after.
      localStorage.removeItem("totalfloat.dates");
      if (height) Object.defineProperty(HTMLElement.prototype, "clientHeight", height);
    }
  });

  it("opens the history panel from its command, and switches between it and the inspector", async () => {
    const { api } = loaded();
    const registry = new Registry();
    render(<Shell api={api} registry={registry} follow={false} />);
    await settle();
    act(() => registry.inOrder().find((action) => action.id === "view.history")?.run());
    await settle();
    // A view over the schedule, its tab chosen; a plain file says there's no history.
    expect(screen.getByRole("region", { name: "History" }).textContent).toContain("has no history");
    expect(screen.getByRole("tab", { name: "History" }).getAttribute("aria-selected")).toBe("true");
    expect(document.querySelector(".workspace")?.hasAttribute("inert")).toBe(true);
    // Explain goes back to the schedule, with the inspector in the right column.
    act(() => registry.inOrder().find((action) => action.id === "explain.open")?.run());
    expect(screen.queryByRole("region", { name: "History" })).toBeNull();
    expect(screen.getByRole("tab", { name: "Schedule" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    act(() => registry.inOrder().find((action) => action.id === "view.panel")?.run());
    expect(screen.queryByRole("complementary")).toBeNull();
    // History stays once opened, hidden, so what it shows in the grid lasts.
    expect(document.querySelector("section.history")?.closest(".view-page")?.hasAttribute("hidden")).toBe(true);
    // A view's key again goes back to the schedule.
    act(() => registry.inOrder().find((action) => action.id === "view.history")?.run());
    expect(screen.getByRole("region", { name: "History" })).toBeTruthy();
    act(() => registry.inOrder().find((action) => action.id === "view.history")?.run());
    expect(screen.queryByRole("region", { name: "History" })).toBeNull();
  });

  it("moves focus into the inspector from its key, and back to the grid", async () => {
    const { api } = loaded();
    const registry = new Registry();
    render(<Shell api={api} registry={registry} follow={false} />);
    await settle();
    const run = (id: string) => act(() => registry.inOrder().find((action) => action.id === id)?.run());
    expect(screen.queryByRole("complementary")).toBeNull();
    run("inspector.focus");
    const inspector = document.querySelector(".right-column .inspector");
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    expect(document.activeElement).toBe(inspector);
    run("inspector.focus");
    expect(document.activeElement).toBe(document.querySelector(".grid-body"));
    // The right column's toggle hides it, and shows it again.
    run("view.panel");
    expect(screen.queryByRole("complementary")).toBeNull();
    run("view.panel");
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    await settle();
  });

  it("shows the schedule in code order when the WBS tree is malformed", async () => {
    const { api } = loaded({ schema: "totalfloat.wbs/1", projects: [{ project: 1, code: "P", nodes: null }] });
    render(<Shell api={api} registry={new Registry()} follow={false} />);
    await settle();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText("Loading activities…")).toBeNull();
    expect(screen.getAllByText("Activity ID").length).toBeGreaterThan(0);
  });

  it("counts the proposals waiting in the title bar, and opens them to review", async () => {
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
    const fetcher: typeof fetch = (input) => {
      const url = String(input);
      if (url.startsWith("/api/1/summary")) {
        return Promise.resolve(
          json({ location: "plant.tfdb", branch: "main", head: "h1", writable: true, calculated: true, inspect: inspectOf("plant.tfdb@main", []) }),
        );
      }
      if (url.startsWith("/api/1/history/proposals")) return Promise.resolve(json(proposals));
      if (url.startsWith("/api/1/rows")) return Promise.resolve(json(sample()));
      if (url.startsWith("/api/1/wbs")) return Promise.resolve(json(NO_WBS));
      if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
      if (url.startsWith("/api/1/history/branch")) return Promise.resolve(json({ branches: [], current: "main", schema: "", store: "" }));
      return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
    };
    render(<Shell api={new Api(null, fetcher)} registry={new Registry()} follow={false} />);
    await settle();
    const badge = screen.getByRole("button", { name: "Proposals to review: 1" });
    act(() => badge.click());
    await settle();
    expect(screen.getByRole("region", { name: "Review" }).textContent).toContain("Crane late");
  });

  it("keeps the app's keys from acting behind the merge dialog, and gives focus back to the branch button", async () => {
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
    const branches = {
      schema: "totalfloat.history-branches/1",
      store: "plant.tfdb",
      current: "main",
      branches: [
        { name: "main", head: "h1", short_id: "h1", current: true, author: "Pat", created: "", last_change: "" },
        { name: "what-if", head: "h2", short_id: "h2", current: false, author: "Pat", created: "", last_change: "" },
      ],
    };
    const fetcher: typeof fetch = (input) => {
      const url = String(input);
      if (url.startsWith("/api/1/summary")) {
        return Promise.resolve(
          json({ location: "plant.tfdb", branch: "main", head: "h1", writable: true, calculated: true, inspect: inspectOf("plant.tfdb@main", []) }),
        );
      }
      if (url.startsWith("/api/1/history/branch")) return Promise.resolve(json(branches));
      if (url.startsWith("/api/1/history/proposals")) return Promise.resolve(json({ proposals: [] }));
      if (url.startsWith("/api/1/rows")) return Promise.resolve(json(sample()));
      if (url.startsWith("/api/1/wbs")) return Promise.resolve(json(NO_WBS));
      if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
      return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
    };
    const registry = new Registry();
    let pressed = 0;
    registry.register({ id: "test.key", name: "Test", group: "View", keys: ["x"], run: () => pressed++ });
    render(<Shell api={new Api(null, fetcher)} registry={registry} follow={false} />);
    await settle();
    act(() => screen.getByRole("button", { name: /Branch/ }).click());
    await settle();
    act(() => screen.getByRole("button", { name: "Merge what-if into main" }).click());
    await settle();
    const dialog = screen.getByRole("dialog", { name: "Merge what-if into main" });
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "x" }));
    });
    expect(pressed).toBe(0);
    act(() => {
      dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(screen.queryByRole("dialog", { name: "Merge what-if into main" })).toBeNull();
    expect(document.activeElement?.classList.contains("branch-button")).toBe(true);
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "x" }));
    });
    expect(pressed).toBe(1);
  });

  it("drops a restore still on its way once a layout is opened, and forgets a deleted one it remembered", async () => {
    const layout = (first: string) => ({
      schema: "totalfloat.layout/1",
      columns: [
        { field: first, width: 120 },
        { field: first === "name" ? "code" : "name", width: 200 },
      ],
      sort: [],
      group: "wbs",
      filters: { text: "", quick: [] },
      gantt: { zoom: { px_per_day: 8, top: "month", bottom: "week" }, lines: "all", today: false },
      split: 0.6,
    });
    let answerOld: (response: Response) => void = () => {};
    const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
    const fetcher: typeof fetch = (input, init) => {
      const url = String(input);
      if (url.startsWith("/api/1/summary")) {
        return Promise.resolve(
          json({ location: "plant.xer", branch: null, head: null, writable: false, calculated: true, inspect: inspectOf("plant.xer", []) }),
        );
      }
      if (init?.method === "DELETE") return Promise.resolve(json({ deleted: true }));
      if (url === "/api/1/layouts/Old") return new Promise((resolve) => (answerOld = resolve));
      if (url === "/api/1/layouts/New") return Promise.resolve(json(layout("name")));
      if (url === "/api/1/layouts" && !init?.method) return Promise.resolve(json({ schema: "totalfloat.layouts/1", layouts: [{ name: "New" }, { name: "Old" }] }));
      if (url.startsWith("/api/1/rows")) return Promise.resolve(json(sample()));
      if (url.startsWith("/api/1/wbs")) return Promise.resolve(json(NO_WBS));
      if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
      return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
    };
    localStorage.setItem("totalfloat.layout:plant.xer", "Old");
    const registry = new Registry();
    render(<Shell api={new Api(null, fetcher)} registry={registry} follow={false} />);
    await settle();
    const run = (id: string) => act(() => registry.inOrder().find((action) => action.id === id)?.run());
    run("layout.open");
    await settle();
    act(() => screen.getByRole("button", { name: /^New/ }).click());
    await settle();
    // The old restore lands late: New stands.
    answerOld(json(layout("code")));
    await settle();
    const titles = () => [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    expect(titles().slice(0, 2)).toEqual(["Activity name", "Activity ID"]);
    expect(localStorage.getItem("totalfloat.layout:plant.xer")).toBe("New");
    // Remembered but never opened: deleting it forgets it.
    localStorage.setItem("totalfloat.layout:plant.xer", "old");
    run("layout.delete");
    await settle();
    act(() => screen.getByRole("button", { name: "Delete layout Old" }).click());
    act(() => screen.getByRole("button", { name: "Delete" }).click());
    await settle();
    expect(localStorage.getItem("totalfloat.layout:plant.xer")).toBeNull();
    localStorage.clear();
  });

  it("keeps the layout chosen last, over a save or restore that answers late, and over the person's own changes", async () => {
    const layout = (first: string) => ({
      schema: "totalfloat.layout/1",
      columns: [
        { field: first, width: 120 },
        { field: first === "name" ? "code" : "name", width: 200 },
      ],
      sort: [],
      group: "wbs",
      filters: { text: "", quick: [] },
      gantt: { zoom: { px_per_day: 8, top: "month", bottom: "week" }, lines: "all", today: false },
      split: 0.6,
    });
    const waiting = new Map<string, (response: Response) => void>();
    let delayOld = false;
    const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
    const fetcher: typeof fetch = (input, init) => {
      const url = String(input);
      const method = init?.method ?? "GET";
      if (url.startsWith("/api/1/summary")) {
        return Promise.resolve(
          json({ location: "plant.xer", branch: null, head: null, writable: false, calculated: true, inspect: inspectOf("plant.xer", []) }),
        );
      }
      if (url === "/api/1/layouts" && method === "GET") return Promise.resolve(json({ schema: "totalfloat.layouts/1", layouts: [{ name: "New" }, { name: "Old" }] }));
      if (url === "/api/1/layouts/New" && method === "GET") return Promise.resolve(json(layout("name")));
      if (url === "/api/1/layouts/Old" && method === "GET" && !delayOld) return Promise.resolve(json(layout("code")));
      if (url.startsWith("/api/1/layouts/")) return new Promise((resolve) => waiting.set(`${method} ${url}`, resolve));
      if (url.startsWith("/api/1/rows")) return Promise.resolve(json(sample()));
      if (url.startsWith("/api/1/wbs")) return Promise.resolve(json(NO_WBS));
      if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
      return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
    };
    const registry = new Registry();
    const run = (id: string) => act(() => registry.inOrder().find((action) => action.id === id)?.run());
    const titles = () => [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);

    // Old is in use; Save sends it, and New is opened before the save answers:
    // New stays in use and remembered.
    render(<Shell api={new Api(null, fetcher)} registry={registry} follow={false} />);
    await settle();
    const open = async (name: string) => {
      run("layout.open");
      await settle();
      act(() => screen.getByRole("button", { name: new RegExp(`^${name}`) }).click());
      await settle();
    };
    await open("Old");
    expect(localStorage.getItem("totalfloat.layout:plant.xer")).toBe("Old");
    run("layout.save");
    await settle();
    expect(waiting.has("PUT /api/1/layouts/Old")).toBe(true);
    await open("New");
    expect(localStorage.getItem("totalfloat.layout:plant.xer")).toBe("New");
    waiting.get("PUT /api/1/layouts/Old")?.(json(layout("code")));
    await settle();
    expect(localStorage.getItem("totalfloat.layout:plant.xer")).toBe("New");
    cleanup();

    // A restore answers after the person has filtered: their filter stands.
    delayOld = true;
    localStorage.setItem("totalfloat.layout:plant.xer", "Old");
    render(<Shell api={new Api(null, fetcher)} registry={new Registry()} follow={false} />);
    await settle();
    const filter = screen.getByRole("searchbox");
    fireEvent.change(filter, { target: { value: "A1" } });
    await settle();
    waiting.get("GET /api/1/layouts/Old")?.(json(layout("code")));
    await settle();
    expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe("A1");
    expect(titles()[0]).toBe("Activity ID");
    expect(titles()[1]).toBe("Activity name");
    localStorage.clear();
  });
});

describe("editing in the shell", () => {
  it("opens the Changes panel on the first edit waiting when no panel is open, keeps the grid's focus, and counts the edits", async () => {
    const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
    localStorage.clear();
    try {
      const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
      const edits: unknown[] = [];
      const fetcher: typeof fetch = (input, init) => {
        const url = String(input);
        if (url.startsWith("/api/1/summary")) {
          return Promise.resolve(
            json({ location: "plant.tfdb", branch: "main", head: "h1", writable: true, calculated: true, inspect: inspectOf("plant.tfdb@main", []) }),
          );
        }
        if (url.startsWith("/api/1/edit")) {
          edits.push(JSON.parse(String(init?.body)));
          return Promise.resolve(json(editReport));
        }
        if (url.startsWith("/api/1/history/proposals")) return Promise.resolve(json({ ...proposals, proposals: [] }));
        if (url.startsWith("/api/1/rows")) return Promise.resolve(json(sample()));
        if (url.startsWith("/api/1/wbs")) return Promise.resolve(json(NO_WBS));
        if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
        if (url.startsWith("/api/1/history/branch")) return Promise.resolve(json({ branches: [], current: "main", schema: "", store: "" }));
        return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
      };
      const registry = new Registry();
      const { container } = render(<Shell api={new Api(null, fetcher)} registry={registry} follow={false} />);
      await settle();
      expect(container.querySelector(".right-column")).toBeNull();
      const body = container.querySelector(".grid-body") as HTMLElement;
      body.focus();
      fireEvent.keyDown(body, { key: "ArrowDown" });
      fireEvent.keyDown(body, { key: "ArrowRight" });
      fireEvent.keyDown(body, { key: "ArrowRight" });
      /** Types `text` in the active cell and commits it with Enter, which moves down. */
      const type = async (text: string) => {
        fireEvent.keyDown(body, { key: text[0] ?? "" });
        const input = container.querySelector(".cell-input") as HTMLInputElement;
        fireEvent.change(input, { target: { value: text } });
        fireEvent.keyDown(input, { key: "Enter" });
        await settle();
      };
      await type("6d");
      expect(screen.getByRole("region", { name: "Changes" })).toBeTruthy();
      // The grid kept its focus, so typing goes on.
      expect(document.activeElement).toBe(body);
      await type("7d");
      expect(edits).toHaveLength(2);
      expect(screen.getByRole("complementary", { name: "Changes" })).toBeTruthy();
      expect(screen.getByRole("group", { name: "Edits pending" }).textContent).toContain("2 edits pending");
      expect(within(screen.getByRole("list", { name: "Edits waiting" })).getAllByRole("listitem")).toHaveLength(2);

      // Applied, the panel that opened by itself closes, and the toggle opens the one chosen before.
      // Applied from the status bar.
      act(() => within(screen.getByRole("group", { name: "Edits pending" })).getByRole("button", { name: "Apply" }).click());
      await settle();
      expect(edits).toHaveLength(3);
      expect((edits[2] as { dry_run?: boolean }).dry_run).toBeUndefined();
      expect(container.querySelector(".right-column")).toBeNull();
      act(() => registry.inOrder().find((action) => action.id === "view.panel")?.run());
      expect(container.querySelector(".inspector")).not.toBeNull();

      // Another panel open stays, and the status bar counts the edit.
      body.focus();
      await type("8d");
      expect(edits).toHaveLength(4);
      expect(container.querySelector(".inspector")).not.toBeNull();
      expect(screen.queryByRole("region", { name: "Changes" })).toBeNull();
      expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
      expect(screen.getByRole("group", { name: "Edits pending" }).textContent).toContain("1 edit pending");
      act(() => within(screen.getByRole("group", { name: "Edits pending" })).getByRole("button", { name: "Review" }).click());
      expect(screen.getByRole("region", { name: "Changes" })).toBeTruthy();
    } finally {
      if (height) Object.defineProperty(HTMLElement.prototype, "clientHeight", height);
      localStorage.clear();
    }
  });
});

/** A server that answers at once: a summary, the sample rows, and `wbs`. */
function loaded(wbs: unknown = NO_WBS, rows = sample()) {
  const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
  const fetcher: typeof fetch = (input) => {
    const url = String(input);
    if (url.startsWith("/api/1/summary")) {
      return Promise.resolve(
        json({
          location: "plant.xer",
          branch: null,
          head: null,
          writable: false,
          calculated: true,
          inspect: inspectOf("plant.xer", []),
        }),
      );
    }
    if (url.startsWith("/api/1/rows")) return Promise.resolve(json(rows));
    if (url.startsWith("/api/1/wbs")) return Promise.resolve(json(wbs));
    if (url.startsWith("/api/1/fields")) return Promise.resolve(json(NO_FIELDS));
    return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
  };
  return { api: new Api(null, fetcher) };
}

describe("what the Gantt lights", () => {
  const lit = (name: number) => ({ ids: new Set([name]), relationships: new Set<number>(), edges: new Set<string>() });
  const [path, highlight, chain] = [lit(1), lit(2), lit(3)];

  it("shows a driving path asked for over the inspector's chain", () => {
    expect(litOf(path, null, chain)).toBe(path);
    expect(litOf(null, null, chain)).toBe(chain);
  });

  it("keeps an opened proposal's changes over a driving path, and under the chain, as before", () => {
    expect(litOf(path, highlight, chain)).toBe(highlight);
    expect(litOf(null, highlight, chain)).toBe(chain);
    expect(litOf(null, highlight, null)).toBe(highlight);
    expect(litOf(null, null, null)).toBeNull();
  });
});

describe("the WBS order", () => {
  it("is read from the server's answer", () => {
    const order = orderOf(golden["wbs-1"]);
    expect(order?.size).toBeGreaterThan(0);
  });
});
