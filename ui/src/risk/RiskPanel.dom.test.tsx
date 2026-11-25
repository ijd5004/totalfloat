// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api, type Rows } from "../api";
import { golden } from "../api/schemas/golden";
import { View } from "../view";
import { RiskPanel, type RiskProject, requestOf, riskMemory, scheduleIdentity } from "./RiskPanel";
import { dateText } from "../dates";

afterEach(() => {
  cleanup();
  memory = riskMemory();
});

const REPORT = golden["risk-analysis-1"];

const ONE: RiskProject[] = [{ id: REPORT.project_id, short_name: REPORT.project }];
const HEAD = "h1";
let memory = riskMemory();
/** The memory the shell would keep; a test's renders share it. */
const MEMORY = () => memory;

const ROWS = {
  schema: "totalfloat.rows/1",
  file: "x.xer",
  count: 2,
  id: [101, 102],
  project: [REPORT.project_id, REPORT.project_id],
  code: ["A1", "A2"],
  name: ["One", "Two"],
} as unknown as Rows;

function viewWithRows(rows: Rows = ROWS): View {
  const view = new View();
  view.publish({
    rows,
    arranged: { lines: Int32Array.from([0, 1]), bands: [], matched: 2 },
    top: 0,
    height: 400,
    rowHeight: 22,
    active: 0,
    bodyTop: 0,
  });
  return view;
}

/** A client whose server answers the golden report, recording what was sent. */
function answering(): { api: Api; sent: unknown[] } {
  const sent: unknown[] = [];
  const api = new Api("token", (_url, init) => {
    sent.push(JSON.parse(String(init?.body ?? "null")));
    return Promise.resolve(new Response(JSON.stringify(REPORT), { status: 200 }));
  });
  return { api, sent };
}

describe("the risk panel", () => {
  it("sends the range, iterations, seed and target, and shows what comes back", async () => {
    const { api, sent } = answering();
    render(<RiskPanel head={HEAD} memory={MEMORY()} api={api} view={viewWithRows()} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    fireEvent.change(screen.getByLabelText("Pessimistic %"), { target: { value: "40" } });
    fireEvent.change(screen.getByLabelText("Finish by"), { target: { value: "2026-02-01 17:00" } });
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(screen.getByText("P80")).toBeTruthy());
    expect(sent).toEqual([
      { inputs: { default: { min: -10, max: 40 } }, iterations: 1000, seed: 1, target: "2026-02-01 17:00" },
    ]);
    const p80 = REPORT.percentiles.find((p) => p.p === 80)?.finish ?? "";
    expect(screen.getByText("P80").nextElementSibling?.textContent).toBe(dateText(p80));
    expect(screen.getByRole("img", { name: /Finish dates from/ })).toBeTruthy();
  });

  it("selects an activity in the grid when it's clicked, then shows the schedule", async () => {
    const { api } = answering();
    const view = viewWithRows();
    const order: string[] = [];
    const reveal = vi.spyOn(view, "reveal").mockImplementation(() => order.push("reveal"));
    const onShow = vi.fn(() => order.push("show"));
    render(<RiskPanel head={HEAD} memory={MEMORY()} api={api} view={view} projects={ONE} onClose={() => {}} onShow={onShow} />);
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    const first = REPORT.criticality[0];
    if (!first) throw new Error("the golden report has no criticality");
    const buttons = await screen.findAllByRole("button", { name: `${first.code} ${first.name}: show it in the schedule` });
    fireEvent.click(buttons[0] as HTMLElement);
    expect(reveal).toHaveBeenCalledWith(ROWS.id[ROWS.code.indexOf(first.code)]);
    expect(onShow).toHaveBeenCalledTimes(1);
    expect(order).toEqual(["reveal", "show"]);
  });

  it("says what's wrong with the form before sending anything", () => {
    expect(requestOf({ min: "-10", max: "25", iterations: "0", seed: "1", target: "", inputs: "" })).toBe(
      "Iterations run from 1 to 10,000.",
    );
    expect(requestOf({ min: "-10", max: "25", iterations: "10", seed: "1", target: "", inputs: "{" })).toBe(
      "The inputs aren't valid JSON.",
    );
    expect(
      requestOf({ min: "-10", max: "25", iterations: "10", seed: "1", target: "", inputs: '{"risks": []}' }),
    ).toEqual({ inputs: { risks: [] }, iterations: 10, seed: 1 });
    // A blank field isn't zero, and a seed past 2^53 can't be sent exactly.
    expect(requestOf({ min: "-10", max: "25", iterations: "10", seed: " ", target: "", inputs: "" })).toMatch(/seed/);
    expect(requestOf({ min: "", max: "25", iterations: "10", seed: "1", target: "", inputs: "" })).toMatch(/range/);
    expect(
      requestOf({ min: "-10", max: "25", iterations: "10", seed: "9007199254740993", target: "", inputs: "" }),
    ).toMatch(/seed/);
  });

  it("shows the server's refusal and its hint", async () => {
    const api = new Api("token", () =>
      Promise.resolve(
        new Response(
          JSON.stringify({
            schema: "totalfloat.error/1",
            error: { code: "risk-refused", message: "A9: no such activity", hint: "docs/guide/risk.md" },
          }),
          { status: 422 },
        ),
      ),
    );
    render(<RiskPanel head={HEAD} memory={MEMORY()} api={api} view={viewWithRows()} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("A9: no such activity");
    expect(alert.textContent).toContain("docs/guide/risk.md");
  });

  it("asks which project when there are several, and sends it", async () => {
    const { api, sent } = answering();
    render(
      <RiskPanel
        head={HEAD}
        memory={MEMORY()}
        api={api}
        view={viewWithRows()}
        projects={[
          { id: 1, short_name: "ONE" },
          { id: 2, short_name: "TWO" },
        ]}
        onClose={() => {}}
        onShow={() => {}}
      />,
    );
    fireEvent.change(screen.getByLabelText("Project"), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(sent.length).toBe(1));
    expect((sent[0] as { project?: number }).project).toBe(2);
  });

  it("sends the first project when the list arrives after the panel opens", async () => {
    const { api, sent } = answering();
    const view = viewWithRows();
    const { rerender } = render(<RiskPanel head={HEAD} memory={MEMORY()} api={api} view={view} projects={[]} onClose={() => {}} onShow={() => {}} />);
    rerender(
      <RiskPanel
        head={HEAD}
        memory={MEMORY()}
        api={api}
        view={view}
        projects={[
          { id: 1, short_name: "ONE" },
          { id: 2, short_name: "TWO" },
        ]}
        onClose={() => {}}
        onShow={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(sent.length).toBe(1));
    expect((sent[0] as { project?: number }).project).toBe(1);
  });

  it("puts results away when the schedule's head moves, and says so for a run dropped", async () => {
    const { api } = answering();
    const view = viewWithRows();
    const { rerender } = render(<RiskPanel head="h1" memory={memory} api={api} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(screen.getByText("P80")).toBeTruthy());
    // The same head again (a resync) keeps them.
    rerender(<RiskPanel head="h1" memory={memory} api={api} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    expect(screen.getByText("P80")).toBeTruthy();
    rerender(<RiskPanel head="h2" memory={memory} api={api} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    expect(screen.queryByText("P80")).toBeNull();
    expect(screen.getByText(/The schedule changed/)).toBeTruthy();

    // A run still going when the head moves is dropped, and the panel says so.
    cleanup();
    memory = riskMemory();
    let answer: (r: Response) => void = () => {};
    const slow = new Api("token", () => new Promise<Response>((done) => (answer = done)));
    const again = render(<RiskPanel head="h1" memory={memory} api={slow} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    again.rerender(<RiskPanel head="h2" memory={memory} api={slow} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    expect(screen.getByText(/The schedule changed/)).toBeTruthy();
    await act(async () => {
      answer(new Response(JSON.stringify(REPORT), { status: 200 }));
      await Promise.resolve();
    });
    expect(screen.queryByText("P80")).toBeNull();
  });

  it("keeps the form and results while closed, and a run finishing meanwhile lands", async () => {
    let answer: (r: Response) => void = () => {};
    const slow = new Api("token", () => new Promise<Response>((done) => (answer = done)));
    const view = viewWithRows();
    render(<RiskPanel head="h1" memory={memory} api={slow} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    fireEvent.change(screen.getByLabelText("Seed"), { target: { value: "42" } });
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    cleanup();
    await act(async () => {
      answer(new Response(JSON.stringify(REPORT), { status: 200 }));
      await Promise.resolve();
      await Promise.resolve();
    });
    render(<RiskPanel head="h1" memory={memory} api={slow} view={view} projects={ONE} onClose={() => {}} onShow={() => {}} />);
    expect((screen.getByLabelText("Seed") as HTMLInputElement).value).toBe("42");
    expect(screen.getByText("P80")).toBeTruthy();
  });

  it("tells two plain files apart, though neither has a head", () => {
    const a = scheduleIdentity({ location: "plant.xer", head: null });
    const b = scheduleIdentity({ location: "usage.xer", head: null });
    expect(a).not.toBe(b);
    expect(scheduleIdentity({ location: "plant.tfdb", head: "h1" })).not.toBe(
      scheduleIdentity({ location: "plant.tfdb", head: "h2" }),
    );
    expect(scheduleIdentity(null)).toBeNull();
  });
});
