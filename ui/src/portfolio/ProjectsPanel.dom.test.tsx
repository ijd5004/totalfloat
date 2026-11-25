// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api } from "../api";
import { golden } from "../api/schemas/golden";
import { dateText } from "../dates";
import { ProjectsPanel, isShown } from "./ProjectsPanel";

afterEach(cleanup);

const PORTFOLIO = golden["portfolio-show-1"];
const CAPACITY = golden["portfolio-capacity-1"];

/** A server answering the golden portfolio and capacity, recording requests. */
function serving(portfolio: typeof PORTFOLIO = PORTFOLIO): { api: Api; asked: string[]; opened: unknown[] } {
  const asked: string[] = [];
  const opened: unknown[] = [];
  const api = new Api("token", (url, init) => {
    asked.push(String(url));
    if (String(url).includes("/portfolio/open")) {
      opened.push(JSON.parse(String(init?.body ?? "null")));
      return Promise.resolve(new Response(JSON.stringify(golden["summary-1"]), { status: 200 }));
    }
    const body = String(url).includes("/capacity") ? CAPACITY : portfolio;
    return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
  });
  return { api, asked, opened };
}

describe("the Projects view", () => {
  it("shows the EPS with each node's figures and its projects", async () => {
    const { api } = serving();
    render(<ProjectsPanel api={api} shown={null} onOpened={() => {}} onClose={() => {}} />);
    const tree = await screen.findByRole("list", { name: "EPS" });
    for (const node of PORTFOLIO.nodes) {
      expect(tree.textContent).toContain(node.name);
    }
    for (const p of PORTFOLIO.projects) {
      expect(screen.getByRole("button", { name: `Open ${p.code} in ${p.file}` })).toBeTruthy();
    }
    const first = PORTFOLIO.nodes[0];
    expect(tree.textContent).toContain(`${first?.totals.activities} activities`);
  });

  it("shows a project with no stored dates as calculated, as the grid does", async () => {
    const { api } = serving();
    render(<ProjectsPanel api={api} shown={null} onOpened={() => {}} onClose={() => {}} />);
    const tree = await screen.findByRole("list", { name: "EPS" });
    // rel-fs-positive's file stores no dates; the server calculated it.
    const plant = PORTFOLIO.projects.find((p) => p.file === "plant.xer");
    expect(plant?.results).toBe("calculated");
    expect(plant?.totals.start).toBe("2026-01-12 08:00");
    const line = screen.getByRole("button", { name: `Open ${plant?.code} in plant.xer` }).parentElement;
    expect(line?.textContent).toContain(`${dateText(plant?.totals.start, false)} to ${dateText(plant?.totals.finish, false)}, 2 critical`);
    expect(tree.textContent).not.toContain("stored results");
  });

  it("says when a project's figures are its file's stored results", async () => {
    // The engine refused plant.xer, so the server rolled up what it stores:
    // no dates at all.
    const stored = structuredClone(PORTFOLIO);
    const plant = stored.projects.find((p) => p.file === "plant.xer");
    if (!plant) throw new Error("the golden portfolio has no plant.xer");
    plant.results = "stored";
    Object.assign(plant.totals, { start: null, finish: null, critical: 0, stored_results: 1 });
    for (const node of stored.nodes) node.totals.stored_results = 1;
    const { api } = serving(stored);
    render(<ProjectsPanel api={api} shown={null} onOpened={() => {}} onClose={() => {}} />);
    const tree = await screen.findByRole("list", { name: "EPS" });
    const line = screen.getByRole("button", { name: `Open ${plant.code} in plant.xer` }).parentElement;
    expect(line?.textContent).toContain("no dates, 0 critical");
    expect(line?.textContent).toContain("the file's stored results, since it couldn't be calculated");
    const other = screen.getByRole("button", { name: /in usage\.xer$/ }).parentElement;
    expect(other?.textContent).not.toContain("stored results");
    expect(tree.querySelector(".eps-name")?.textContent).toContain(
      "includes the stored results of 1 project that couldn't be calculated",
    );
  });

  it("opens a project, then has the grid read it", async () => {
    const { api, opened } = serving();
    const onOpened = vi.fn();
    render(<ProjectsPanel api={api} shown={null} onOpened={onOpened} onClose={() => {}} />);
    const last = PORTFOLIO.projects[PORTFOLIO.projects.length - 1];
    if (!last) throw new Error("the golden portfolio has no project");
    fireEvent.click(await screen.findByRole("button", { name: `Open ${last.code} in ${last.file}` }));
    await waitFor(() => expect(onOpened).toHaveBeenCalled());
    expect(opened).toEqual([{ file: last.file }]);
  });

  it("draws the capacity chart for a resource, and asks again by week", async () => {
    const { api, asked } = serving();
    render(<ProjectsPanel api={api} shown={null} onOpened={() => {}} onClose={() => {}} />);
    const chart = await screen.findByRole("img", { name: /demand by week against supply/ });
    expect(chart.querySelectorAll(".capacity-bar").length).toBe(CAPACITY.resources[0]?.periods.length);
    fireEvent.change(screen.getByLabelText("By"), { target: { value: "week" } });
    await waitFor(() => expect(asked.some((u) => u.includes("by=week"))).toBe(true));
  });

  it("marks only the file shown, not one whose name ends the same", () => {
    expect(isShown("/work/subplant.xer", "plant.xer")).toBe(false);
    expect(isShown("/work/plant.xer", "plant.xer")).toBe(true);
    expect(isShown("C:\\work\\plant.xer", "plant.xer")).toBe(true);
    expect(isShown("/work/plant.xer", "./plant.xer")).toBe(true);
    expect(isShown(null, "plant.xer")).toBe(false);
  });
});
