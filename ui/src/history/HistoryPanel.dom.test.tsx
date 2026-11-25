// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Api, type Summary } from "../api";
import baselinesAnswer from "../api/golden/history-baselines-1.json";
import baselineAnswer from "../api/golden/history-baseline-1.json";
import branchAnswer from "../api/golden/history-branch-1.json";
import branchesAnswer from "../api/golden/history-branches-1.json";
import changeset from "../api/golden/changeset-1.json";
import diffAnswer from "../api/golden/history-diff-1.json";
import logAnswer from "../api/golden/history-log-1.json";
import revertAnswer from "../api/golden/history-revert-1.json";
import { sample } from "../edit/fixture";
import { type Compared, comparedKey } from "../grid/columns";
import { View } from "../view";
import { HistoryPanel } from "./HistoryPanel";
import { inspectOf } from "../api/testing";

afterEach(cleanup);

// jsdom lays nothing out, so it has no scrollIntoView; each call is noted on the element.
const scrolled: { element: Element; options: unknown }[] = [];
beforeEach(() => {
  scrolled.length = 0;
  Element.prototype.scrollIntoView = function (this: Element, options?: boolean | ScrollIntoViewOptions) {
    scrolled.push({ element: this, options });
  };
});

const HEAD = logAnswer.head;
const EDIT = logAnswer.changesets[0]?.id ?? "";

interface Asked {
  method: string;
  url: string;
  body: Record<string, unknown> | null;
}

/** A server answering each history request with its golden example, noting what it was asked. */
function server(
  options: {
    diff?: (url: string) => Promise<unknown>;
    log?: unknown;
    logAll?: unknown;
    logOf?: Record<string, unknown>;
    show?: unknown;
    branches?: unknown;
  } = {},
) {
  const asked: Asked[] = [];
  const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "x-totalfloat-head": HEAD } });
  const fetcher: typeof fetch = (input, init) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : null;
    asked.push({ method, url, body });
    if (url.startsWith("/api/1/history/log")) {
      const of = new URL(url, "http://x").searchParams.get("branch");
      if (url.includes("all=true") && options.logAll === "fail") {
        return Promise.resolve(new Response(JSON.stringify({ code: "store-damaged", message: "Broken", hint: "" }), { status: 500 }));
      }
      const answer = url.includes("all=true") ? options.logAll : of ? options.logOf?.[of] : undefined;
      return Promise.resolve(json(answer ?? options.log ?? logAnswer));
    }
    if (url.startsWith("/api/1/history/show/")) return Promise.resolve(json(options.show ?? changeset));
    if (url.startsWith("/api/1/history/diff")) return options.diff ? options.diff(url).then(json) : Promise.resolve(json(diffAnswer));
    if (url.startsWith("/api/1/history/revert")) return Promise.resolve(json(revertAnswer));
    if (url.startsWith("/api/1/history/branch")) {
      return Promise.resolve(json(body?.action === "list" ? (options.branches ?? branchesAnswer) : branchAnswer));
    }
    if (url.startsWith("/api/1/history/baseline")) return Promise.resolve(json(body?.action === "list" ? baselinesAnswer : baselineAnswer));
    if (url.startsWith("/api/1/history/export")) {
      return Promise.resolve(
        new Response("ERMHDR", { status: 200, headers: { "content-disposition": 'attachment; filename="plant-0101010101.xer"' } }),
      );
    }
    return Promise.resolve(new Response("{}", { status: 404 }));
  };
  return { api: new Api(null, fetcher), asked };
}

const SUMMARY: Summary = {
  schema: "totalfloat.summary/1",
  location: "plant.tfdb",
  branch: "main",
  head: HEAD,
  writable: true,
  calculated: true,
  inspect: inspectOf("plant.tfdb@main", [{ id: 7, short_name: "rel-fs-positive", name: null, data_date: null, finish: null, activities: 3 }]),
};

async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
}

async function panel(summary: Summary = SUMMARY, options: Parameters<typeof server>[0] = {}) {
  const { api, asked } = server(options);
  const view = new View();
  const reveal = vi.spyOn(view, "reveal");
  const said: string[] = [];
  const compared: (Compared | null)[] = [];
  const onChanged = vi.fn(() => Promise.resolve());
  const element = (shown: Summary, proposed?: unknown) => (
    <HistoryPanel
      api={api}
      summary={shown}
      proposed={proposed}
      rows={sample()}
      view={view}
      onClose={() => {}}
      onChanged={onChanged}
      say={(text) => said.push(text)}
      onCompared={(c) => compared.push(c)}
    />
  );
  const { rerender } = render(element(summary));
  await settle();
  return {
    asked,
    reveal,
    said,
    compared,
    onChanged,
    show: (next: Summary, proposed?: unknown) => rerender(element(next, proposed)),
  };
}

const entry = (text: string) => screen.getByRole("button", { name: new RegExp(text) });

describe("the history panel", () => {
  it("lists the branch's changes newest first, with who, when and what kind", async () => {
    await panel();
    const items = within(screen.getByRole("list", { name: "Changes, newest first" })).getAllByRole("listitem");
    expect(items.map((item) => item.querySelector(".history-message")?.textContent)).toEqual(["Crane late", "As submittedimport"]);
    expect(items[0]?.textContent).toContain("Pat");
    expect(items[0]?.textContent).toContain("0101010101");
    expect(items[0]?.querySelector(".history-summary")?.textContent).toBe("A2 duration");
  });

  it("titles a change with no reason by what it did, in words, in the log, the compare lists and the revert prompt", async () => {
    const [edit, ...rest] = logAnswer.changesets;
    const log = { ...logAnswer, changesets: [{ ...edit, message: "" }, ...rest] };
    await panel(SUMMARY, { log });
    const items = within(screen.getByRole("list", { name: "Changes, newest first" })).getAllByRole("listitem");
    expect(items[0]?.querySelector(".history-message")?.textContent).toBe("A2 duration");
    // The summary is the title, so it isn't said again beneath it.
    expect(items[0]?.querySelector(".history-summary")).toBeNull();
    for (const list of ["Compare from", "Compare to"]) {
      const labels = [...(screen.getByLabelText(list) as HTMLSelectElement).options].map((option) => option.textContent);
      expect(labels).toContain("0101010101 · A2 duration");
    }
    fireEvent.click(entry("A2 duration"));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Revert…" }));
    expect(screen.getByText(/records a new change that undoes/).textContent).toMatch(/^Taking out A2 duration records/);
    // No command names anywhere a change is named.
    expect(document.body.textContent).not.toMatch(/\b[a-z]+_[a-z_]+ A\d/);
  });

  it("shows what a chosen change did: its edits and the finish it gave", async () => {
    await panel();
    fireEvent.click(entry("Crane late"));
    await settle();
    expect(screen.getByText("A2 duration: original 8h to 32h, remaining 8h to 32h")).toBeTruthy();
    expect(screen.getByText("Finish moved from 13 Jan 26 to 15 Jan 26.")).toBeTruthy();
  });

  it("takes a change out only once a reason is given, after saying what it does", async () => {
    const { asked, said, onChanged } = await panel();
    fireEvent.click(entry("Crane late"));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Revert…" }));
    expect(screen.getByText(/records a new change that undoes/)).toBeTruthy();
    const take = screen.getByRole("button", { name: "Take it out" });
    expect((take as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Why take it out"), { target: { value: "Crane came early" } });
    fireEvent.click(take);
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/revert")?.body).toEqual({ at: EDIT, reason: "Crane came early", head: HEAD });
    expect(onChanged).toHaveBeenCalled();
    expect(said).toContain("Took out 0101010101, as 0505050505.");
  });

  it("doesn't offer to take out the import that starts the history", async () => {
    await panel();
    fireEvent.click(entry("As submitted"));
    await settle();
    expect((screen.getByRole("button", { name: "Revert…" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("branches and baselines from a chosen change", async () => {
    const { asked, said } = await panel();
    fireEvent.click(entry("Crane late"));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Branch from here…" }));
    fireEvent.change(screen.getByLabelText("New branch's name"), { target: { value: "what-if" } });
    fireEvent.click(screen.getByRole("button", { name: "Create branch" }));
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/branch" && a.body?.action === "create")?.body).toEqual({
      action: "create",
      name: "what-if",
      at: EDIT,
    });
    expect(said.at(-1)).toContain("Created branch what-if at 0101010101");
    fireEvent.click(screen.getByRole("button", { name: "Baseline from here…" }));
    fireEvent.change(screen.getByLabelText("Baseline's name"), { target: { value: "submitted" } });
    fireEvent.click(screen.getByRole("button", { name: "Create baseline" }));
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/baseline" && a.body?.action === "create")?.body).toEqual({
      action: "create",
      name: "submitted",
      at: EDIT,
    });
  });

  it("compares a change with now, selects a changed activity, and shows it in the grid when asked", async () => {
    const { asked, reveal, compared } = await panel();
    fireEvent.click(entry("Crane late"));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Compare with now" }));
    await settle();
    expect(asked.find((a) => a.url.startsWith("/api/1/history/diff"))?.url).toBe(`/api/1/history/diff?from=${EDIT}&to=main&limit=0`);
    const result = screen.getByLabelText("Comparison");
    expect(result.textContent).toContain("1 activity changed, 2 became critical");
    // Spans as the grid shows them: hours here, as the sample rows don't say how long a day is.
    expect(result.textContent).toContain("Original duration: 8 h → 32 h");
    fireEvent.click(within(result).getByRole("button", { name: /A2/ }));
    expect(reveal).toHaveBeenCalledWith(2);
    expect(compared.at(-1)).toBeNull();
    fireEvent.click(screen.getByLabelText("Show Before and After in the grid"));
    await settle();
    const shown = compared.at(-1);
    expect(shown?.projects.get(7)).toBe("rel-fs-positive");
    expect(shown?.activities.get(comparedKey("rel-fs-positive", "A2"))?.fields.map((f) => [f.field, f.from, f.to])).toEqual([
      ["original_duration", "8h", "32h"],
      ["remaining_duration", "8h", "32h"],
    ]);
  });

  it("brings the comparison into view, below the log, when a change's or a baseline's Compare with now is pressed", async () => {
    await panel();
    fireEvent.click(entry("Crane late"));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Compare with now" }));
    // The Compare section at once, then its answer once it comes.
    expect(scrolled.map((s) => [s.element.textContent, s.options])).toEqual([["Compare", { block: "start" }]]);
    await settle();
    expect(scrolled.at(-1)?.element).toBe(screen.getByLabelText("Comparison"));
    expect(scrolled.at(-1)?.options).toEqual({ block: "nearest" });

    scrolled.length = 0;
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    expect(scrolled.map((s) => s.element)).toEqual([screen.getByRole("heading", { name: "Compare" }), screen.getByLabelText("Comparison")]);
  });

  it("doesn't scroll to a comparison asked again because the head moved", async () => {
    const { show } = await panel();
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    scrolled.length = 0;
    show({ ...SUMMARY, head: "0303030303030303030303030303030303030303030303030303030303030303" });
    await settle();
    expect(scrolled).toEqual([]);
  });

  it("compares any two points chosen", async () => {
    const { asked } = await panel();
    fireEvent.change(screen.getByLabelText("Compare from"), { target: { value: "submitted" } });
    fireEvent.change(screen.getByLabelText("Compare to"), { target: { value: "what-if" } });
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    await settle();
    expect(asked.find((a) => a.url.startsWith("/api/1/history/diff"))?.url).toBe("/api/1/history/diff?from=submitted&to=what-if&limit=0");
  });

  it("asks before deleting a baseline", async () => {
    const { asked } = await panel();
    fireEvent.click(screen.getByRole("button", { name: "Delete baseline submitted" }));
    expect(asked.some((a) => a.body?.action === "delete")).toBe(false);
    fireEvent.click(within(screen.getByRole("list", { name: "Baselines" })).getByRole("button", { name: "Delete" }));
    await settle();
    expect(asked.find((a) => a.body?.action === "delete")?.body).toEqual({ action: "delete", name: "submitted" });
  });

  it("exports a point as its XER", async () => {
    const { asked, said } = await panel();
    URL.createObjectURL = vi.fn(() => "blob:x");
    URL.revokeObjectURL = vi.fn();
    fireEvent.click(entry("Crane late"));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Export XER" }));
    await settle();
    expect(asked.find((a) => a.url.startsWith("/api/1/history/export"))?.url).toBe(`/api/1/history/export?at=${EDIT}`);
    expect(said).toContain("Exported plant-0101010101.xer.");
  });

  it("says a plain file has no history", async () => {
    await panel({ ...SUMMARY, branch: null, head: null, writable: false });
    expect(screen.getByText(/has no history/)).toBeTruthy();
  });

  it("shows only the latest comparison asked for, whatever order the answers come in", async () => {
    const waiting: ((answer: unknown) => void)[] = [];
    const other = structuredClone(diffAnswer);
    const first = other.diff.projects[0];
    if (first?.project) first.project.code = "OLDER";
    await panel(SUMMARY, { diff: () => new Promise((resolve) => waiting.push(resolve)) });
    fireEvent.change(screen.getByLabelText("Compare from"), { target: { value: "submitted" } });
    fireEvent.change(screen.getByLabelText("Compare to"), { target: { value: "main" } });
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    fireEvent.change(screen.getByLabelText("Compare from"), { target: { value: "what-if" } });
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    await settle();
    expect(waiting).toHaveLength(2);
    waiting[1]?.(diffAnswer);
    await settle();
    waiting[0]?.(other);
    await settle();
    expect(screen.getByLabelText("Comparison").textContent).not.toContain("OLDER");
    expect(screen.getByLabelText("Comparison").textContent).toContain("1 activity changed");
  });

  it("asks a comparison again once the head moves", async () => {
    const { asked, show } = await panel();
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    const diffs = () => asked.filter((a) => a.url.startsWith("/api/1/history/diff")).map((a) => a.url);
    expect(diffs()).toEqual(["/api/1/history/diff?from=submitted&to=main&limit=0"]);
    show({ ...SUMMARY, head: "f".repeat(64) });
    await settle();
    expect(diffs()).toEqual(["/api/1/history/diff?from=submitted&to=main&limit=0", "/api/1/history/diff?from=submitted&to=main&limit=0"]);
  });

  it("offers older changes when the log is as long as asked for", async () => {
    const many = { ...logAnswer, changesets: Array.from({ length: 200 }, (_, k) => ({ ...logAnswer.changesets[0], id: `${k}`.padStart(64, "0"), short_id: `${k}` })) };
    const { asked } = await panel(SUMMARY, { log: many });
    expect(asked.find((a) => a.url.startsWith("/api/1/history/log"))?.url).toBe("/api/1/history/log?limit=200");
    fireEvent.click(screen.getByRole("button", { name: "Show older changes" }));
    await settle();
    // Every branch is listed now, so the branch's own listing is one of several asked for.
    expect(asked.some((a) => a.url === "/api/1/history/log?limit=400")).toBe(true);
  });

  it("doesn't select an activity of a project the schedule shown doesn't have", async () => {
    const elsewhere = structuredClone(diffAnswer);
    const first = elsewhere.diff.projects[0];
    if (first?.project) first.project.code = "Q";
    const { reveal, said } = await panel(SUMMARY, { diff: () => Promise.resolve(elsewhere) });
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    fireEvent.click(within(screen.getByLabelText("Comparison")).getByRole("button", { name: /A2/ }));
    expect(reveal).not.toHaveBeenCalled();
    expect(said.at(-1)).toBe("A2 isn't in the schedule shown.");
  });

  it("asks for every changed activity, and shows all of them in the grid", async () => {
    const many = structuredClone(diffAnswer);
    const first = many.diff.projects[0];
    const template = first?.changed?.[0];
    if (!first || !template) throw new Error("the golden example changes an activity");
    first.changed = Array.from({ length: 30 }, (_, k) => ({ ...structuredClone(template), code: `X${k}` }));
    if (first.counts) first.counts.changed = 30;
    const { asked, compared } = await panel(SUMMARY, { diff: () => Promise.resolve(many) });
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    expect(asked.find((a) => a.url.startsWith("/api/1/history/diff"))?.url).toContain("limit=0");
    expect(screen.getByLabelText("Comparison").textContent).not.toContain("Only the first");
    fireEvent.click(screen.getByLabelText("Show Before and After in the grid"));
    await settle();
    const code = first.project?.code ?? "";
    const shown = compared.at(-1);
    expect([...(shown?.activities.keys() ?? [])]).toHaveLength(30);
    expect(shown?.activities.has(comparedKey(code, "X29"))).toBe(true);
  });

  it("says when a comparison lists fewer than it counted", async () => {
    const many = structuredClone(diffAnswer);
    const first = many.diff.projects[0];
    if (first?.counts) first.counts.changed = 40;
    await panel(SUMMARY, { diff: () => Promise.resolve(many) });
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    expect(screen.getByLabelText("Comparison").textContent).toContain("Only the first 25 of each kind are listed");
  });

  it("lists branches and baselines again on each reload, even where the head didn't move", async () => {
    const { asked, show } = await panel();
    const lists = () => asked.filter((a) => a.body?.action === "list").length;
    const before = lists();
    show({ ...SUMMARY });
    await settle();
    expect(lists()).toBe(before + 2);
  });

  it("drops a comparison still on its way when the head moves, and asks again", async () => {
    const waiting: ((answer: unknown) => void)[] = [];
    const older = structuredClone(diffAnswer);
    const first = older.diff.projects[0];
    if (first?.project) first.project.code = "OLDER";
    const { show } = await panel(SUMMARY, { diff: () => new Promise((resolve) => waiting.push(resolve)) });
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    show({ ...SUMMARY, head: "f".repeat(64) });
    await settle();
    expect(waiting).toHaveLength(2);
    waiting[0]?.(older);
    await settle();
    expect(screen.queryByLabelText("Comparison")).toBeNull();
    waiting[1]?.(diffAnswer);
    await settle();
    expect(screen.getByLabelText("Comparison").textContent).not.toContain("OLDER");
  });

  it("compares with the branch shown now, after a switch", async () => {
    const { asked, show } = await panel();
    fireEvent.click(screen.getByRole("button", { name: "Compare baseline submitted with now" }));
    await settle();
    show({ ...SUMMARY, branch: "what-if" });
    await settle();
    const diffs = asked.filter((a) => a.url.startsWith("/api/1/history/diff")).map((a) => a.url);
    expect(diffs).toEqual(["/api/1/history/diff?from=submitted&to=main&limit=0", "/api/1/history/diff?from=submitted&to=what-if&limit=0"]);
  });

  it("lists proposals, waiting and decided, when asked, with who decided and why", async () => {
    const decided = {
      ...changeset,
      state: "rejected",
      reviews: [{ state: "rejected", author: "Pat", author_kind: "human", message: "Crane is on time", created: "2026-11-02T10:00:00Z" }],
    };
    // `all` lists every branch's changes: only proposals are taken from it.
    // The other branch's change is listed from its own branch, for the graph.
    const proposal = { ...logAnswer.changesets[0], id: "9".repeat(64), short_id: "9999999999", state: "rejected", message: "Pour later", created: "2026-11-03T09:00:00Z" };
    const otherBranch = { ...logAnswer.changesets[0], id: "8".repeat(64), short_id: "8888888888", message: "On what-if" };
    const { asked } = await panel(SUMMARY, {
      show: decided,
      logAll: { ...logAnswer, changesets: [proposal, otherBranch, ...logAnswer.changesets] },
      logOf: { "what-if": { ...logAnswer, branch: "what-if", changesets: [otherBranch, ...logAnswer.changesets] } },
    });
    fireEvent.click(screen.getByLabelText("Show proposals, waiting and decided"));
    await settle();
    expect(asked.some((a) => a.url === "/api/1/history/log?limit=200&all=true")).toBe(true);
    const messages = [...document.querySelectorAll(".history-entry .history-message")].map((m) => m.textContent);
    expect(messages).toEqual(["Pour laterrejected", "On what-if", "Crane late", "As submittedimport"]);
    // A proposal isn't on the branch: no revert, branch or baseline from it.
    fireEvent.click(entry("Pour later"));
    await settle();
    expect(screen.queryByRole("button", { name: "Revert…" })).toBeNull();
    expect(screen.getByRole("button", { name: "Compare with now" })).toBeTruthy();
    fireEvent.click(entry("Crane late"));
    await settle();
    const decisions = screen.getByRole("list", { name: "Decisions" });
    expect(decisions.textContent).toContain("Rejected by Pat");
    expect(decisions.textContent).toContain(": Crane is on time");
  });

  it("lists again, and reads the chosen change again, when proposals change or the page reloads", async () => {
    const { asked, show } = await panel();
    fireEvent.click(entry("Crane late"));
    await settle();
    const count = (start: string) => asked.filter((a) => a.url.startsWith(start)).length;
    const [logs, shows] = [count("/api/1/history/log"), count("/api/1/history/show/")];
    // A proposal made or decided elsewhere: a new listing, the head unmoved.
    show(SUMMARY, [{ id: "new" }]);
    await settle();
    // Each listing asks for the branch's log, every change, and each other branch's log.
    expect(count("/api/1/history/log")).toBe(logs * 2);
    expect(count("/api/1/history/show/")).toBe(shows + 1);
    // A reload: a new summary, the same head.
    show({ ...SUMMARY }, [{ id: "new" }]);
    await settle();
    expect(count("/api/1/history/show/")).toBe(shows + 2);
  });

  it("keeps a proposal accepted by replay in the history, with its decision", async () => {
    // Accepted, but replayed as another change: the original is on no branch.
    const original = { ...logAnswer.changesets[0], id: "7".repeat(64), short_id: "7777777777", message: "Lift later", state: "accepted", created: "2026-11-03T09:00:00Z" };
    const decided = {
      ...changeset,
      id: original.id,
      reviews: [{ state: "accepted", author: "Pat", author_kind: "human", message: "Agreed", created: "2026-11-03T10:00:00Z" }],
    };
    await panel(SUMMARY, {
      show: decided,
      logAll: { ...logAnswer, changesets: [original, ...logAnswer.changesets] },
      logOf: { "what-if": logAnswer },
    });
    fireEvent.click(screen.getByLabelText("Show proposals, waiting and decided"));
    await settle();
    expect(entry("Lift later").textContent).toContain("accepted proposal");
    fireEvent.click(entry("Lift later"));
    await settle();
    expect(screen.getByRole("list", { name: "Decisions" }).textContent).toContain("Accepted by Pat");
    expect(screen.queryByRole("button", { name: "Revert…" })).toBeNull();
  });

  it("offers older changes when the listing of proposals is cut short, even if the branch's isn't", async () => {
    const many = Array.from({ length: 200 }, (_, k) => ({ ...logAnswer.changesets[0], id: `${k}`.padStart(64, "0"), short_id: `${k}` }));
    // Every branch's own listing is short; only the listing of every change is cut.
    await panel(SUMMARY, { logAll: { ...logAnswer, changesets: many }, logOf: { "what-if": logAnswer } });
    expect(screen.queryByRole("button", { name: "Show older changes" })).toBeNull();
    fireEvent.click(screen.getByLabelText("Show proposals, waiting and decided"));
    await settle();
    expect(screen.getByRole("button", { name: "Show older changes" })).toBeTruthy();
  });

  it("doesn't count an edit left by a deleted branch as a proposal", async () => {
    // On no branch, accepted, and never decided: the edit of a branch since deleted.
    const leftover = { ...logAnswer.changesets[0], id: "6".repeat(64), short_id: "6666666666", message: "On a deleted branch", created: "2026-11-03T09:00:00Z" };
    await panel(SUMMARY, {
      show: changeset,
      logAll: { ...logAnswer, changesets: [leftover, ...logAnswer.changesets] },
      logOf: { "what-if": logAnswer },
    });
    fireEvent.click(screen.getByLabelText("Show proposals, waiting and decided"));
    await settle();
    const messages = [...document.querySelectorAll(".history-entry .history-message")].map((m) => m.textContent);
    expect(messages).toEqual(["Crane late", "As submittedimport"]);
  });
});

/** A change as the log lists it. */
function change(name: string, parent: string | null, more: Record<string, unknown> = {}) {
  return {
    ...logAnswer.changesets[0],
    id: name.repeat(64),
    short_id: name.repeat(10),
    message: `Change ${name}`,
    parent: parent ? parent.repeat(64) : null,
    second_parent: null,
    ...more,
  };
}

/**
 * main: i, a, b, c; what-if started at a, holding w and x; x merged into main
 * as m; a proposal p waiting on main.
 */
function merged() {
  const [i, a, b, c] = [change("i", null, { kind: "import" }), change("a", "i"), change("b", "a"), change("c", "b")];
  const [w, x] = [change("w", "a"), change("x", "w")];
  const m = change("m", "c", { kind: "merge", second_parent: "x".repeat(64) });
  const p = change("p", "m", { state: "proposed" });
  const branch = (name: string, head: string, current: boolean) => ({ ...branchesAnswer.branches[0], name, head: head.repeat(64), current });
  return {
    log: { ...logAnswer, head: m.id, changesets: [m, c, b, a, i] },
    logAll: { ...logAnswer, branch: null, head: null, changesets: [p, m, x, w, c, b, a, i] },
    logOf: { "what-if": { ...logAnswer, branch: "what-if", head: x.id, changesets: [x, w, a, i] } },
    branches: { ...branchesAnswer, branches: [branch("main", "m", true), branch("what-if", "x", false)] },
  };
}

const dotOf = (id: string) => document.querySelector(`.history-dot[data-id="${id}"]`);
const dot = (name: string) => dotOf(name.repeat(64));
const rowOf = (name: string) => dot(name)?.closest("li") ?? null;

describe("the history graph", () => {
  it("draws each branch as a line, joined at a merge, with its name at its head", async () => {
    await panel({ ...SUMMARY, head: "m".repeat(64) }, merged());
    const messages = [...document.querySelectorAll(".history-entry .history-message")].map((m) => m.textContent);
    expect(messages).toEqual(["Change mmerge", "Change x", "Change w", "Change c", "Change b", "Change a", "Change iimport"]);
    // Two lanes: main's changes in the first, what-if's in the second.
    const lane = (name: string) => dot(name)?.getAttribute("cx");
    expect(new Set(["m", "c", "b", "a", "i"].map(lane))).toEqual(new Set(["7"]));
    expect(new Set(["x", "w"].map(lane))).toEqual(new Set(["19"]));
    // The merge leaves for what-if's lane, and what-if's line comes into main at a, where it started.
    expect(rowOf("m")?.querySelectorAll("path")).toHaveLength(1);
    expect(rowOf("a")?.querySelector("path")?.getAttribute("d")).toMatch(/^M 19 0/);
    expect(rowOf("c")?.querySelector("path")).toBeNull();
    // Both names at their heads.
    expect(rowOf("m")?.querySelector(".history-refs")?.textContent).toBe("main");
    expect(rowOf("x")?.querySelector(".history-refs")?.textContent).toBe("what-if");
  });

  it("shows a proposal waiting as a hollow dot off its branch, when proposals are shown", async () => {
    await panel({ ...SUMMARY, head: "m".repeat(64) }, merged());
    expect(dot("p")).toBeNull();
    fireEvent.click(screen.getByLabelText("Show proposals, waiting and decided"));
    await settle();
    expect(dot("p")?.classList.contains("hollow")).toBe(true);
    expect(dot("p")?.getAttribute("cx")).toBe("31");
    expect(dot("m")?.classList.contains("hollow")).toBe(false);
  });

  it("selects a change when its dot is clicked, as clicking its row does", async () => {
    await panel();
    const edit = dotOf(EDIT);
    if (!edit) throw new Error("the edit has a dot");
    expect(document.querySelector(".history-entry[aria-pressed='true']")).toBeNull();
    fireEvent.click(edit);
    await settle();
    expect(screen.getAllByRole("button", { pressed: true })).toHaveLength(1);
    expect(entry("Crane late").getAttribute("aria-pressed")).toBe("true");
    expect(edit.classList.contains("chosen")).toBe(true);
    expect(screen.getByText("A2 duration: original 8h to 32h, remaining 8h to 32h")).toBeTruthy();
    // Clicking the row and clicking the dot do the same: here, close it again.
    fireEvent.click(edit);
    expect(entry("Crane late").getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(entry("Crane late"));
    expect(edit.classList.contains("chosen")).toBe(true);
  });

  it("draws a store with only main as one straight line", async () => {
    await panel(SUMMARY, { branches: { ...branchesAnswer, branches: [branchesAnswer.branches[0]] } });
    const dots = [...document.querySelectorAll(".history-dot")];
    expect(dots).toHaveLength(2);
    expect(new Set(dots.map((d) => d.getAttribute("cx")))).toEqual(new Set(["7"]));
    expect(document.querySelectorAll(".history-graph path")).toHaveLength(0);
    expect(new Set([...document.querySelectorAll(".history-graph line")].map((l) => l.getAttribute("x1")))).toEqual(new Set(["7"]));
    // The list is as it was beside it.
    const items = within(screen.getByRole("list", { name: "Changes, newest first" })).getAllByRole("listitem");
    expect(items.map((item) => item.querySelector(".history-message")?.textContent)).toEqual(["Crane late", "As submittedimport"]);
  });

  it("labels a change with the baselines taken at it", async () => {
    await panel();
    const labels = [...(dotOf(EDIT)?.closest("li")?.querySelectorAll(".history-ref") ?? [])];
    expect(labels.map((l) => [l.className, l.textContent])).toEqual([
      ["history-ref branch tone-0", "main"],
      ["history-ref branch tone-0", "what-if"],
      ["history-ref baseline", "submitted"],
    ]);
    expect(entry("As submitted").querySelector(".history-ref")).toBeNull();
  });

  it("takes out only a change on the branch shown's own line", async () => {
    await panel({ ...SUMMARY, head: "m".repeat(64) }, merged());
    fireEvent.click(entry("Change w"));
    await settle();
    const revert = screen.getByRole("button", { name: "Revert…" }) as HTMLButtonElement;
    expect(revert.disabled).toBe(true);
    expect(revert.title).toContain("main's own line");
    // Branching from it is still offered.
    expect((screen.getByRole("button", { name: "Branch from here…" }) as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(entry("Change c"));
    await settle();
    expect((screen.getByRole("button", { name: "Revert…" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("offers older changes when a merge reaches back past the changes listed", async () => {
    // what-if was deleted after its merge, and the listing of every change stops before w.
    const store = merged();
    const m = store.log.changesets[0];
    const x = store.logOf["what-if"].changesets[0];
    const filler = Array.from({ length: 198 }, (_, k) => change("f", null, { id: `f${k}`.padEnd(64, "0") }));
    await panel(
      { ...SUMMARY, head: "m".repeat(64) },
      {
        ...store,
        logAll: { ...store.logAll, changesets: [m, x, ...filler] },
        logOf: {},
        branches: { ...store.branches, branches: [store.branches.branches[0]] },
      },
    );
    expect(dot("x")).toBeTruthy();
    expect(dot("w")).toBeNull();
    expect(screen.getByRole("button", { name: "Show older changes" })).toBeTruthy();
  });

  it("still draws the branches when every change can't be listed", async () => {
    await panel({ ...SUMMARY, head: "m".repeat(64) }, { ...merged(), logAll: "fail" });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(dot("m")?.getAttribute("cx")).toBe("7");
    expect(dot("x")?.getAttribute("cx")).toBe("19");
  });

  it("keeps the changes of a branch deleted after it was merged", async () => {
    const store = merged();
    await panel({ ...SUMMARY, head: "m".repeat(64) }, { ...store, logOf: {}, branches: { ...store.branches, branches: [store.branches.branches[0]] } });
    expect(dot("x")?.getAttribute("cx")).toBe("19");
    expect(dot("w")?.classList.contains("tone-none")).toBe(true);
    expect(rowOf("x")?.querySelector(".history-refs")).toBeNull();
  });
});
