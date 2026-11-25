// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api, type HistoryLog, type Summary } from "../api";
import diffAnswer from "../api/golden/history-diff-1.json";
import logAnswer from "../api/golden/history-log-1.json";
import changeset from "../api/golden/changeset-1.json";
import mergeAnswer from "../api/golden/history-merge-1.json";
import { sample } from "../edit/fixture";
import { View } from "../view";
import { MergeDialog } from "./MergeDialog";
import { inspectOf } from "../api/testing";

afterEach(cleanup);

type Entry = HistoryLog["changesets"][number];
const entry = (id: string, summary: string, message = ""): Entry => ({ ...(logAnswer.changesets[0] as Entry), id, short_id: id.slice(0, 10), summary, message });

const [BASE, OURS, THEIRS] = ["b".repeat(64), "1".repeat(64), "3".repeat(64)];

const SUMMARY: Summary = {
  schema: "totalfloat.summary/1",
  location: "plant.tfdb",
  branch: "main",
  head: "h1",
  writable: true,
  calculated: true,
  inspect: inspectOf("plant.tfdb@main", [{ id: 7, short_name: "rel-fs-positive", name: null, data_date: null, finish: null, activities: 3 }]),
};

const CLASH = {
  source: "project 7",
  code: "both-changed",
  message: "A2 original_duration: this branch has 64h, and the other has 56h",
  hint: "change one side to match, or take the change out of one, then merge again",
};

async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
}

/** The dry run's answer for a clean merge: the change it would record, here the golden diff's. */
const PREVIEW = { ...mergeAnswer, replayed: [THEIRS], impact: diffAnswer.diff };

/**
 * The dialog, with `merge` answering the merge and `preview` its dry run
 * (by default a clean merge's, with the golden diff as its impact).
 */
function dialog(merge: () => Response, preview?: () => Promise<Response>) {
  const asked: { url: string; body: Record<string, unknown> | null }[] = [];
  const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
  const fetcher: typeof fetch = (input, init) => {
    const url = String(input);
    const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : null;
    asked.push({ url, body });
    if (url.startsWith("/api/1/history/diff")) return Promise.resolve(json(diffAnswer));
    if (url.startsWith("/api/1/history/merge") && body?.dry_run) return preview ? preview() : Promise.resolve(json(PREVIEW));
    if (url.startsWith("/api/1/history/merge")) return Promise.resolve(merge());
    if (url.startsWith("/api/1/history/log")) {
      const branch = new URL(url, "http://x").searchParams.get("branch");
      const base = entry(BASE, "import plant.xer");
      const changes =
        branch === "main"
          ? [entry(OURS, "A2 duration", "Crane late"), base]
          : [entry(THEIRS, "A2 duration"), base];
      return Promise.resolve(json({ ...logAnswer, branch, head: changes[0]?.id, changesets: changes }));
    }
    if (url.startsWith("/api/1/history/show/")) {
      const id = decodeURIComponent(url.slice("/api/1/history/show/".length));
      // Theirs was recorded with no reason: it's named by its summary.
      const message = id === OURS ? "Crane late" : id === THEIRS ? "" : "import";
      const edits = id === BASE ? [] : [{ op: "set_duration", activity: "A2", original: id === OURS ? "8d" : "7d" }];
      return Promise.resolve(
        json({
          ...changeset,
          id,
          short_id: id.slice(0, 10),
          parent: id === BASE ? null : BASE,
          second_parent: null,
          message,
          summary: id === BASE ? "import plant.xer" : "A2 duration",
          commands: { edits, resolved: [{ id: 2, kind: "activity", name: "A2", project: 7 }] },
        }),
      );
    }
    return Promise.resolve(json({}, 404));
  };
  const view = new View();
  const reveal = vi.spyOn(view, "reveal");
  const said: string[] = [];
  const onClose = vi.fn();
  const onChanged = vi.fn(() => Promise.resolve());
  const api = new Api(null, fetcher);
  const element = (summary: Summary) => (
    <MergeDialog
      api={api}
      summary={summary}
      rows={sample()}
      view={view}
      from="what-if"
      onClose={onClose}
      onChanged={onChanged}
      say={(text) => said.push(text)}
      onCompared={() => {}}
    />
  );
  const { rerender } = render(element(SUMMARY));
  return { asked, reveal, said, onClose, onChanged, json, show: (summary: Summary) => rerender(element(summary)) };
}

describe("the merge dialog", () => {
  it("previews first what the merge would record, from its dry run", async () => {
    const { asked } = dialog(() => new Response("{}"));
    await settle();
    expect(screen.getByRole("dialog", { name: "Merge what-if into main" })).toBeTruthy();
    expect(asked[0]).toEqual({ url: "/api/1/history/merge", body: { branch: "what-if", dry_run: true, head: "h1" } });
    // The heads aren't compared: that would show main's later work as undone.
    expect(asked.some((a) => a.url.startsWith("/api/1/history/diff"))).toBe(false);
    expect(screen.getByText("What the merge would change on main")).toBeTruthy();
    expect(screen.getByLabelText("Comparison").textContent).toContain("1 activity changed");
  });

  it("previews a fast-forward as the other branch's head", async () => {
    const forward = { ...mergeAnswer, fast_forward: true };
    const { asked } = dialog(
      () => new Response("{}"),
      () => Promise.resolve(new Response(JSON.stringify(forward))),
    );
    await settle();
    expect(asked.map((a) => a.url)).toContain("/api/1/history/diff?from=main&to=what-if");
    expect(screen.getByText("main has no changes of its own, so merging moves it to where what-if is")).toBeTruthy();
    expect(screen.getByLabelText("Comparison").textContent).toContain("1 activity changed");
  });

  it("says when there's nothing to merge, and offers no merge", async () => {
    dialog(
      () => new Response("{}"),
      () => Promise.resolve(new Response(JSON.stringify(mergeAnswer))),
    );
    await settle();
    expect(screen.getByText("main already has everything on what-if, so there's nothing to merge.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Merge what-if into main" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("merges, saying which change it recorded, and refreshes the view", async () => {
    const merged = { ...mergeAnswer, recorded: { id: "5".repeat(64), short_id: "5555555555", branch: "main" } };
    const { asked, said, onClose, onChanged } = dialog(() => new Response(JSON.stringify(merged)));
    await settle();
    fireEvent.change(screen.getByLabelText("Message (optional)"), { target: { value: "Take the faster pour" } });
    fireEvent.click(screen.getByRole("button", { name: "Merge what-if into main" }));
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/merge" && !a.body?.dry_run)?.body).toEqual({
      branch: "what-if",
      reason: "Take the faster pour",
      head: "h1",
    });
    expect(onChanged).toHaveBeenCalled();
    expect(said.at(-1)).toBe("Merged what-if into main as 5555555555.");
    expect(onClose).toHaveBeenCalled();
  });

  it("lists each clash with both values and the changes that set them, selects its activity, and changes nothing", async () => {
    const refused = {
      schema: "totalfloat.error/1",
      error: { code: "merge-conflict", message: "what-if can't be merged as it stands, so nothing was recorded", hint: "", problems: [CLASH] },
    };
    const { reveal, onChanged, onClose } = dialog(() => new Response(JSON.stringify(refused), { status: 409 }));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Merge what-if into main" }));
    await settle();
    const clashes = within(screen.getByRole("list", { name: "Clashes" }));
    const clash = clashes.getByRole("button", { name: /A2/ });
    // The values from the comparison of the heads, in hours here as the
    // sample rows don't say how long A2's day is; the changes from each side.
    expect(clash.textContent).toContain("main: 8 h (1111111111 Crane late)");
    expect(clash.textContent).toContain("what-if: 32 h (3333333333 A2 duration)");
    fireEvent.click(clash);
    expect(reveal).toHaveBeenCalledWith(2);
    expect(onChanged).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText("Nothing was changed on either branch.")).toBeTruthy();
  });

  it("lists the clashes the dry run finds without merging, and offers no merge", async () => {
    const refused = {
      schema: "totalfloat.error/1",
      error: { code: "merge-conflict", message: "what-if can't be merged as it stands, so nothing was recorded", hint: "", problems: [CLASH] },
    };
    const { asked, onChanged } = dialog(
      () => new Response("{}"),
      () => Promise.resolve(new Response(JSON.stringify(refused), { status: 409 })),
    );
    await settle();
    const clash = within(screen.getByRole("list", { name: "Clashes" })).getByRole("button", { name: /A2/ });
    expect(clash.textContent).toContain("main: 8 h (1111111111 Crane late)");
    expect(clash.textContent).toContain("what-if: 32 h (3333333333 A2 duration)");
    expect(screen.getByText("Nothing was changed on either branch.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Merge what-if into main" }) as HTMLButtonElement).disabled).toBe(true);
    expect(asked.filter((a) => a.url === "/api/1/history/merge" && !a.body?.dry_run)).toHaveLength(0);
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("closes on Escape", async () => {
    const { onClose } = dialog(() => new Response("{}"));
    await settle();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("says when the merge only moved the branch forward", async () => {
    const forward = { ...mergeAnswer, fast_forward: true, recorded: null };
    const { said } = dialog(() => new Response(JSON.stringify(forward)));
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Merge what-if into main" }));
    await settle();
    expect(said.at(-1)).toBe("main had no changes of its own, so it now points where what-if does.");
  });

  it("previews again when the branch moves, and merges only on a preview of it as it is", async () => {
    const waiting: ((response: Response) => void)[] = [];
    const merged = { ...mergeAnswer, recorded: { id: "5".repeat(64), short_id: "5555555555", branch: "main" } };
    const { asked, show } = dialog(
      () => new Response(JSON.stringify(merged)),
      () => new Promise((resolve) => waiting.push(resolve)),
    );
    await settle();
    waiting[0]?.(new Response(JSON.stringify(PREVIEW)));
    await settle();
    const merge = () => screen.getByRole("button", { name: "Merge what-if into main" }) as HTMLButtonElement;
    expect(merge().disabled).toBe(false);
    // Main moves on: the preview shown is of h1, so merging waits for one of h2.
    show({ ...SUMMARY, head: "h2" });
    await settle();
    expect(waiting).toHaveLength(2);
    expect(merge().disabled).toBe(true);
    waiting[1]?.(new Response(JSON.stringify(PREVIEW)));
    await settle();
    expect(merge().disabled).toBe(false);
    fireEvent.click(merge());
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/merge" && !a.body?.dry_run)?.body).toEqual({ branch: "what-if", head: "h2" });
  });

  it("keeps focus inside while it's open", async () => {
    const outside = document.createElement("button");
    document.body.append(outside);
    dialog(() => new Response("{}"));
    await settle();
    act(() => outside.focus());
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
    outside.remove();
  });

  it("waits for a new preview when only the other branch moved", async () => {
    const waiting: ((response: Response) => void)[] = [];
    const { show } = dialog(
      () => new Response("{}"),
      () => new Promise((resolve) => waiting.push(resolve)),
    );
    await settle();
    waiting[0]?.(new Response(JSON.stringify(PREVIEW)));
    await settle();
    const merge = () => screen.getByRole("button", { name: "Merge what-if into main" }) as HTMLButtonElement;
    expect(merge().disabled).toBe(false);
    // A reload with main where it was: what-if may have moved, so the preview is asked again.
    show({ ...SUMMARY });
    await settle();
    expect(waiting).toHaveLength(2);
    expect(merge().disabled).toBe(true);
  });
});
