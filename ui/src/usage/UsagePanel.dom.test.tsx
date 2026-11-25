// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { type Api, ApiError, type UsageReport } from "../api";
import type { Change } from "../edit/editing";
import { UsagePanel, limitText } from "./UsagePanel";

afterEach(cleanup);

function report(by: "week" | "month"): UsageReport {
  return {
    schema: "totalfloat.usage/1",
    file: "plan.xer",
    resource: { short_name: "CREW", name: "Civil crew" },
    by,
    unit: "hours",
    limits: [{ from: null, units_per_hour: 1 }],
    assignments: 2,
    not_spread: [],
    totals: { actual: 20, remaining: 70, total: 90 },
    periods: [
      { start: "2026-01-05", actual: 20, remaining: 10, total: 30, limit: 40, over: false },
      { start: "2026-01-12", actual: 0, remaining: 60, total: 60, limit: 40, over: true },
    ].slice(0, by === "month" ? 1 : 2),
  };
}

/** An API that answers usage for CREW and refuses anything else. */
function api(asked: { resource: string; by?: string }[]): Api {
  return {
    usage: (query: { resource: string; by?: "week" | "month" }) => {
      asked.push(query);
      if (query.resource !== "CREW") {
        return Promise.reject(new ApiError(404, "no-such-resource", "no resource matches NOPE", "check the short name"));
      }
      return Promise.resolve(report(query.by ?? "week"));
    },
  } as unknown as Api;
}

async function settle() {
  await act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
}

describe("the usage panel", () => {
  it("shows a resource's periods as stacked bars and a table, marking the one over its limit", async () => {
    const asked: { resource: string; by?: string }[] = [];
    const { container } = render(
      <UsagePanel api={api(asked)} resources={[{ short_name: "CREW", name: "Civil crew" }]} head="a" onClose={() => {}} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    expect(asked).toEqual([{ resource: "CREW", by: "week" }]);
    expect(container.querySelectorAll(".usage-period")).toHaveLength(2);
    expect(container.querySelectorAll(".usage-period.usage-over")).toHaveLength(1);
    expect(container.querySelector(".usage-limit")?.getAttribute("d")).toBe("M0 46.7 H160 V46.7 H320");
    const rows = [...container.querySelectorAll(".usage-table tbody tr")].map((tr) => tr.textContent);
    expect(rows).toEqual(["05 Jan 26201040", "12 Jan 2606040 over"]);
    expect(container.querySelector(".usage-summary")?.textContent).toContain("over 2 weeks; over its limit in 1");
  });

  it("asks again by month, and says why a resource can't be shown", async () => {
    const asked: { resource: string; by?: string }[] = [];
    render(<UsagePanel api={api(asked)} resources={[]} head="a" onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(screen.getByRole("alert").textContent).toBe("Choose a resource.");
    fireEvent.change(screen.getByLabelText("Resource"), { target: { value: "CREW" } });
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    fireEvent.change(screen.getByLabelText("By"), { target: { value: "month" } });
    await settle();
    expect(asked.at(-1)).toEqual({ resource: "CREW", by: "month" });
    expect(screen.getByRole("columnheader", { name: "Month" })).toBeTruthy();
    expect(document.querySelector(".usage-summary")?.textContent).toContain("over 1 month.");
    fireEvent.change(screen.getByLabelText("Resource"), { target: { value: "NOPE" } });
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    expect(screen.getByRole("alert").textContent).toContain("no resource matches NOPE");
  });

  it("asks again when the schedule shown changes", async () => {
    const asked: { resource: string; by?: string }[] = [];
    const props = { api: api(asked), resources: [{ short_name: "CREW", name: "Civil crew" }], onClose: () => {} };
    const { rerender } = render(<UsagePanel {...props} head="a" />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    rerender(<UsagePanel {...props} head="b" />);
    await settle();
    expect(asked).toHaveLength(2);
  });

  // The field follows the schedule's resources.
  it("starts over from the new list when the new schedule has no such resource", async () => {
    const asked: { resource: string; by?: string }[] = [];
    const props = { api: api(asked), onClose: () => {} };
    const { container, rerender } = render(<UsagePanel {...props} resources={[]} head="a" />);
    expect((screen.getByLabelText("Resource") as HTMLInputElement).value).toBe("");
    rerender(<UsagePanel {...props} resources={[{ short_name: "CREW", name: "Civil crew" }]} head="a" />);
    expect((screen.getByLabelText("Resource") as HTMLInputElement).value).toBe("CREW");
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    rerender(<UsagePanel {...props} resources={[{ short_name: "ELEC", name: "Electricians" }]} head="b" />);
    await settle();
    expect(asked).toEqual([{ resource: "CREW", by: "week" }]);
    expect((screen.getByLabelText("Resource") as HTMLInputElement).value).toBe("ELEC");
    expect(container.querySelector(".usage-table")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("asks by the new period for the resource in the field", async () => {
    const asked: { resource: string; by?: string }[] = [];
    render(<UsagePanel api={api(asked)} resources={[{ short_name: "CREW", name: "Civil crew" }]} head="a" onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    fireEvent.change(screen.getByLabelText("Resource"), { target: { value: "ELEC" } });
    fireEvent.change(screen.getByLabelText("By"), { target: { value: "month" } });
    await settle();
    expect(asked.at(-1)).toEqual({ resource: "ELEC", by: "month" });
  });

  // An answer still on its way is asked for again, and the late answer is dropped.
  it("asks again for a new schedule or period while the first answer is on its way", async () => {
    const asked: { resource: string; by?: string }[] = [];
    const answers: ((report: UsageReport) => void)[] = [];
    const slow = {
      usage: (query: { resource: string; by?: "week" | "month" }) => {
        asked.push(query);
        return new Promise<UsageReport>((answer) => answers.push(answer));
      },
    } as unknown as Api;
    const props = { api: slow, resources: [{ short_name: "CREW", name: "Civil crew" }], onClose: () => {} };
    const { container, rerender } = render(<UsagePanel {...props} head="a" />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    rerender(<UsagePanel {...props} head="b" />);
    fireEvent.change(screen.getByLabelText("By"), { target: { value: "month" } });
    expect(asked).toEqual([
      { resource: "CREW", by: "week" },
      { resource: "CREW", by: "week" },
      { resource: "CREW", by: "month" },
    ]);
    // The month answers first; the two week answers come after it and are dropped.
    await act(async () => {
      answers[2]?.(report("month"));
      await new Promise((done) => setTimeout(done, 0));
    });
    await act(async () => {
      answers[0]?.(report("week"));
      answers[1]?.(report("week"));
      await new Promise((done) => setTimeout(done, 0));
    });
    expect(screen.getByRole("columnheader", { name: "Month" })).toBeTruthy();
    expect(container.querySelectorAll(".usage-table tbody tr")).toHaveLength(1);
  });

  it("lists every assignment that couldn't be spread", async () => {
    const many = { ...report("week"), not_spread: Array.from({ length: 7 }, (_, k) => ({ activity: `A${k}`, reason: "no dates" })) };
    const props = { resources: [{ short_name: "CREW", name: "Civil crew" }], head: "a", onClose: () => {} };
    const api = { usage: () => Promise.resolve(many) } as unknown as Api;
    const { container } = render(<UsagePanel api={api} {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    expect(container.querySelector(".usage-not-spread summary")?.textContent).toBe("7 assignments couldn't be spread");
    expect(container.querySelectorAll(".usage-not-spread li")).toHaveLength(7);
  });

  it("says the limit in force, and sets or clears it as an edit waiting with the others", async () => {
    const changes: Change[] = [];
    render(
      <UsagePanel
        api={api([])}
        resources={[{ short_name: "CREW", name: "Civil crew" }]}
        head="a"
        onClose={() => {}}
        limits={{ enabled: true, change: (c) => changes.push(c) }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    expect(document.querySelector(".usage-limit-now")?.textContent).toBe("Limit 1 unit an hour.");
    // Set, throughout.
    fireEvent.change(screen.getByLabelText("Limit, units an hour"), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Set limit" }));
    expect(changes.at(-1)).toEqual({
      key: "change:limit:CREW@",
      edit: { op: "set_resource_limit", resource: "CREW", max_units_per_hour: 2 },
      words: "Limit CREW to 2 units an hour",
    });
    // Set from a day.
    fireEvent.change(screen.getByLabelText("Limit, units an hour"), { target: { value: "3.5" } });
    fireEvent.change(screen.getByLabelText("From"), { target: { value: "2027-01-04" } });
    fireEvent.click(screen.getByRole("button", { name: "Set limit" }));
    expect(changes.at(-1)?.edit).toEqual({ op: "set_resource_limit", resource: "CREW", max_units_per_hour: 3.5, from: "2027-01-04" });
    expect(changes.at(-1)?.words).toBe("Limit CREW to 3.5 units an hour from 04 Jan 27");
    // Cleared.
    fireEvent.change(screen.getByLabelText("From"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Clear limit" }));
    expect(changes.at(-1)?.edit).toEqual({ op: "set_resource_limit", resource: "CREW", max_units_per_hour: null });
    expect(changes.at(-1)?.words).toBe("Clear CREW's limit");
    // A limit that isn't a number waits for a fix.
    for (const typo of ["-1", "0x10", "1e3", ""]) {
      fireEvent.change(screen.getByLabelText("Limit, units an hour"), { target: { value: typo } });
      fireEvent.click(screen.getByRole("button", { name: "Set limit" }));
      expect(screen.getByRole("alert").textContent).toContain("zero or more units an hour");
    }
    expect(changes).toHaveLength(3);
  });

  it("offers no form where the schedule is read-only", async () => {
    render(
      <UsagePanel
        api={api([])}
        resources={[{ short_name: "CREW", name: "Civil crew" }]}
        head="a"
        onClose={() => {}}
        limits={{ enabled: false, change: () => {} }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await settle();
    expect(document.querySelector(".usage-limit-now")?.textContent).toBe("Limit 1 unit an hour.");
    expect(screen.queryByRole("button", { name: "Set limit" })).toBeNull();
  });

  it("words a limit that changes over time", () => {
    expect(limitText([])).toBe("No limit");
    expect(limitText([{ from: null, units_per_hour: null }])).toBe("No limit");
    expect(
      limitText([
        { from: null, units_per_hour: 2 },
        { from: "2027-01-04 00:00", units_per_hour: 3 },
        { from: "2027-03-01 00:00", units_per_hour: null },
      ]),
    ).toBe("Limit 2 units an hour; 3 from 04 Jan 27; none from 01 Mar 27");
    expect(limitText([{ from: null, units_per_hour: 1.001 }])).toBe("Limit 1 unit an hour");
    expect(limitText([{ from: null, units_per_hour: null }, { from: "2027-01-04 00:00", units_per_hour: 1 }])).toBe(
      "Limit none; 1 from 04 Jan 27",
    );
  });
});
