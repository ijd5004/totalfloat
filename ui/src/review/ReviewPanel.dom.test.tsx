// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api, type Summary } from "../api";
import changeset from "../api/golden/changeset-1.json";
import diffAnswer from "../api/golden/history-diff-1.json";
import proposalsAnswer from "../api/golden/history-proposals-1.json";
import reviewAnswer from "../api/golden/history-review-1.json";
import { sample } from "../edit/fixture";
import type { DrivingPath } from "../gantt/draw";
import type { Compared } from "../grid/columns";
import { View } from "../view";
import { type Proposal, ReviewPanel } from "./ReviewPanel";
import { inspectOf } from "../api/testing";

afterEach(cleanup);

const PROPOSALS = proposalsAnswer.proposals as Proposal[];
const PROPOSAL = PROPOSALS[0] as Proposal;
// As the server sends them: `who` already says it's an agent.
const AGENTS: Proposal[] = [{ ...PROPOSAL, author: "planner-bot", author_kind: "agent", who: "planner-bot (agent)" }];

const SUMMARY: Summary = {
  schema: "totalfloat.summary/1",
  location: "plant.tfdb",
  branch: "main",
  head: PROPOSAL.parent ?? "h",
  writable: true,
  calculated: true,
  inspect: inspectOf("plant.tfdb@main", [{ id: 7, short_name: "rel-fs-positive", name: null, data_date: null, finish: null, activities: 3 }]),
};

interface Asked {
  url: string;
  body: Record<string, unknown> | null;
}

/** A server with golden answers; `accept` says how an accept is answered. */
function server(accept: () => Response = () => json(reviewAnswer)) {
  const asked: Asked[] = [];
  const fetcher: typeof fetch = (input, init) => {
    const url = String(input);
    const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : null;
    asked.push({ url, body });
    if (url.startsWith("/api/1/history/show/")) return Promise.resolve(json(changeset));
    if (url.startsWith("/api/1/history/diff")) return Promise.resolve(json(diffAnswer));
    if (url.startsWith("/api/1/history/accept")) return Promise.resolve(accept());
    if (url.startsWith("/api/1/history/reject")) return Promise.resolve(json({ ...reviewAnswer, state: "rejected" }));
    return Promise.resolve(new Response("{}", { status: 404 }));
  };
  return { api: new Api(null, fetcher), asked };
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status });
}

async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
}

async function panel(options: { proposals?: Proposal[] | null; summary?: Summary; accept?: () => Response } = {}) {
  const { api, asked } = server(options.accept);
  const view = new View();
  const said: string[] = [];
  const compared: (Compared | null)[] = [];
  const lit: (DrivingPath | null)[] = [];
  const onChanged = vi.fn(() => Promise.resolve());
  const { unmount } = render(
    <ReviewPanel
      api={api}
      summary={options.summary ?? SUMMARY}
      rows={sample()}
      view={view}
      proposals={options.proposals === undefined ? AGENTS : options.proposals}
      onClose={() => {}}
      onChanged={onChanged}
      say={(text) => said.push(text)}
      onCompared={(c) => compared.push(c)}
      onHighlight={(p) => lit.push(p)}
    />,
  );
  await settle();
  return { asked, said, compared, lit, onChanged, unmount };
}

const open = async () => {
  fireEvent.click(screen.getByRole("button", { name: /Crane late/ }));
  await settle();
};

describe("the review panel", () => {
  it("lists what waits: why, who (an agent marked), when, for which branch, and its impact", async () => {
    await panel();
    const item = within(screen.getByRole("list", { name: "Proposals waiting" })).getByRole("listitem");
    expect(item.textContent).toContain("Crane late");
    expect(item.textContent).toContain("planner-bot (agent) ·");
    expect(item.textContent).not.toContain("(agent) (agent)");
    expect(item.textContent).toContain("for main");
    expect(item.textContent).toContain("rel-fs-positive finish 2026-01-15 09:00 to 2026-01-15 17:00");
  });

  it("opens a proposal: its edits in words, its impact against where it was made, lit in the grid and Gantt", async () => {
    const { asked, compared, lit } = await panel();
    await open();
    expect(screen.getByText("A2 duration: original 8h to 32h, remaining 8h to 32h")).toBeTruthy();
    expect(asked.find((a) => a.url.startsWith("/api/1/history/diff"))?.url).toBe(
      `/api/1/history/diff?from=${PROPOSAL.parent}&to=${PROPOSAL.id}`,
    );
    expect(screen.getByLabelText("Comparison").textContent).toContain("1 activity changed");
    expect(compared.at(-1)?.activities.size).toBe(1);
    expect([...(lit.at(-1)?.ids ?? [])]).toEqual([2]);
  });

  it("accepts a proposal onto its branch as it is now, with no head guard to refuse a branch that moved on", async () => {
    const { asked, said, onChanged } = await panel();
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/accept")?.body).toEqual({ proposal: PROPOSAL.id });
    expect(onChanged).toHaveBeenCalled();
    expect(said.at(-1)).toBe("Accepted 0404040404 onto main.");
  });

  it("says when an accept was replayed on a branch that moved on", async () => {
    const { said } = await panel({ accept: () => json({ ...reviewAnswer, replayed: true, recorded: "0909090909090909" }) });
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await settle();
    expect(said.at(-1)).toBe("Accepted 0404040404: main had moved on, so it was replayed on its head as 0909090909.");
  });

  it("lists what no longer applies when an accept is refused, and changes nothing", async () => {
    const refused = {
      schema: "totalfloat.error/1",
      error: {
        code: "accept-conflict",
        message: "accepting 0404040404: changeset 0404040404 no longer applies, so nothing was recorded",
        hint: "each problem says what it needed",
        problems: [{ source: "edit 1", code: "no-activity", message: "there's no activity A2", hint: "it was deleted" }],
      },
    };
    const { said, onChanged } = await panel({ accept: () => json(refused, 409) });
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await settle();
    const conflicts = screen.getByRole("alert");
    expect(conflicts.textContent).toContain("no longer applies");
    expect(within(conflicts).getByRole("list", { name: "Conflicts" }).textContent).toContain("there's no activity A2");
    expect(onChanged).not.toHaveBeenCalled();
    expect(said).toEqual([]);
  });

  it("rejects only with a reason", async () => {
    const { asked, said } = await panel();
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Reject…" }));
    const reject = screen.getByRole("button", { name: "Reject" });
    expect((reject as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Why reject it"), { target: { value: "Crane is on time" } });
    fireEvent.click(reject);
    await settle();
    expect(asked.find((a) => a.url === "/api/1/history/reject")?.body).toEqual({
      proposal: PROPOSAL.id,
      reason: "Crane is on time",
    });
    expect(said.at(-1)).toBe("Rejected 0404040404.");
  });

  it("shows a refusal without problems as a refusal, not a conflict", async () => {
    const refused = { schema: "totalfloat.error/1", error: { code: "read-only", message: "Not here", hint: "serve a branch", problems: [] } };
    const { said } = await panel({ accept: () => json(refused, 403) });
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await settle();
    expect(screen.queryByRole("list", { name: "Conflicts" })).toBeNull();
    expect(said.at(-1)).toBe("Not here");
  });

  it("says when its edits couldn't be read, rather than reading forever", async () => {
    const { api } = server();
    const failing = new Api(null, (input, init) =>
      String(input).startsWith("/api/1/history/show/")
        ? Promise.resolve(json({ schema: "totalfloat.error/1", error: { code: "not-found", message: "No such changeset", hint: "" } }, 404))
        : (api as unknown as { fetcher: typeof fetch }).fetcher(input, init),
    );
    render(
      <ReviewPanel
        api={failing}
        summary={SUMMARY}
        rows={sample()}
        view={new View()}
        proposals={AGENTS}
        onClose={() => {}}
        onChanged={() => Promise.resolve()}
        say={() => {}}
        onCompared={() => {}}
        onHighlight={() => {}}
      />,
    );
    await open();
    expect(screen.getByText("No such changeset")).toBeTruthy();
  });

  it("clears a cancelled reason", async () => {
    await panel();
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Reject…" }));
    fireEvent.change(screen.getByLabelText("Why reject it"), { target: { value: "wrong crew" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Reject…" }));
    expect((screen.getByLabelText("Why reject it") as HTMLInputElement).value).toBe("");
  });

  it("clears the grid and Gantt when it closes", async () => {
    const { compared, lit, unmount } = await panel();
    await open();
    unmount();
    expect(compared.at(-1)).toBeNull();
    expect(lit.at(-1)).toBeNull();
  });

  it("says when nothing waits, and that a plain file has no proposals", async () => {
    await panel({ proposals: [] });
    expect(screen.getByText(/Nothing waiting/)).toBeTruthy();
    cleanup();
    await panel({ summary: { ...SUMMARY, head: null, branch: null } });
    expect(screen.getByText(/A plain file has no proposals/)).toBeTruthy();
  });

  it("keeps an accept's conflicts with its own proposal when another is opened meanwhile", async () => {
    let answer: (response: Response) => void = () => {};
    const refused = {
      schema: "totalfloat.error/1",
      error: {
        code: "accept-conflict",
        message: "no longer applies",
        hint: "",
        problems: [{ source: "edit 1", code: "no-activity", message: "there's no activity A2", hint: "" }],
      },
    };
    const second: Proposal = { ...PROPOSAL, id: "5".repeat(64), short_id: "5555555555", reason: "Pour later", author: "planner-bot", author_kind: "agent" };
    const { api } = server();
    const deferred = new Api(null, (input, init) =>
      String(input).startsWith("/api/1/history/accept")
        ? new Promise<Response>((resolve) => (answer = resolve))
        : (api as unknown as { fetcher: typeof fetch }).fetcher(input, init),
    );
    render(
      <ReviewPanel
        api={deferred}
        summary={SUMMARY}
        rows={sample()}
        view={new View()}
        proposals={[...AGENTS, second]}
        onClose={() => {}}
        onChanged={() => Promise.resolve()}
        say={() => {}}
        onCompared={() => {}}
        onHighlight={() => {}}
      />,
    );
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    fireEvent.click(screen.getByRole("button", { name: /Pour later/ }));
    await settle();
    answer(json(refused, 409));
    await settle();
    // The other proposal shows no conflicts that aren't its own.
    expect(screen.getByRole("button", { name: /Pour later/ }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.queryByRole("list", { name: "Conflicts" })).toBeNull();
  });
});
