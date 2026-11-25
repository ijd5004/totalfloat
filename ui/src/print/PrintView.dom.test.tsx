// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { Summary } from "../api";
import { sample } from "../edit/fixture";
import { HEADER_HEIGHT, MARGIN, ROW_HEIGHT, TITLE_HEIGHT, pageSize, rowsPerPage } from "./pages";
import { PrintView } from "./PrintView";
import { inspectOf } from "../api/testing";

afterEach(cleanup);

const SUMMARY: Summary = {
  schema: "totalfloat.summary/1",
  location: "plant.tfdb",
  branch: "main",
  head: "0123456789abcdef",
  writable: true,
  calculated: true,
  inspect: inspectOf("plant.tfdb@main", [{ id: 7, short_name: "PLANT", name: "Plant upgrade", data_date: "2026-01-05 08:00", finish: "2026-02-27 17:00", activities: 300 }]),
};

/** 300 activities, one a working day apart, from the sample's first. */
function many() {
  const base = sample();
  const n = 300;
  const each = <T,>(f: (k: number) => T) => Array.from({ length: n }, (_, k) => f(k));
  const day = (k: number) => {
    const d = new Date(Date.UTC(2026, 0, 5 + k));
    return d.toISOString().slice(0, 10);
  };
  return {
    ...base,
    count: n,
    id: each((k) => k + 1),
    project: each(() => 7),
    code: each((k) => `A${k + 1}`),
    name: each((k) => `Activity number ${k + 1}`),
    type: each(() => "task"),
    status: each(() => "not-started"),
    critical: each((k) => k % 2 === 0),
    calendar: each(() => "Five day"),
    wbs_path: each(() => "P"),
    original_duration_hours: each(() => 8),
    remaining_duration_hours: each(() => 8),
    total_float_hours: each(() => 0),
    free_float_hours: each(() => 0),
    early_start: each((k) => `${day(k)} 08:00`),
    early_finish: each((k) => `${day(k)} 17:00`),
    late_start: each(() => null),
    late_finish: each((k) => `${day(k + 2)} 17:00`),
    actual_start: each(() => null),
    actual_finish: each(() => null),
    constraint: each(() => null),
    constraint_date: each(() => null),
  } as unknown as ReturnType<typeof sample>;
}

describe("the print view", () => {
  it("prints whole rows on pages, each with the title block, headers, legend and its number", () => {
    const rows = many();
    render(
      <PrintView
        rows={rows}
        lines={Int32Array.from({ length: rows.count }, (_, k) => k)}
        bands={[]}
        columns={[
          { id: "code", width: 90 },
          { id: "name", width: 260 },
          { id: "start", width: 116 },
        ]}
        split={0.5}
        summary={SUMMARY}
        onClose={() => {}}
      />,
    );
    const per = rowsPerPage(pageSize("letter", "landscape").height);
    const pages = screen.getAllByRole("group", { name: /^Page \d+ of \d+$/ });
    expect(pages).toHaveLength(Math.ceil(300 / per));
    const bottom = pageSize("letter", "landscape").height - MARGIN;
    let printed = 0;
    pages.forEach((page, p) => {
      const text = page.textContent ?? "";
      expect(text).toContain("Plant upgrade (PLANT)");
      expect(text).toContain("main @ 0123456789");
      expect(text).toContain("Data date 05 Jan 26");
      expect(text).toContain("Activity ID");
      expect(text).toContain("Critical");
      expect(text).toContain(`Page ${p + 1} of ${pages.length}`);
      // No row runs past the body.
      const rowsHere = page.querySelectorAll(".print-row");
      printed += rowsHere.length;
      rowsHere.forEach((row) => {
        const rect = row.querySelector("line");
        const y = Number(rect?.getAttribute("y1"));
        expect(y).toBeGreaterThan(MARGIN + TITLE_HEIGHT + HEADER_HEIGHT);
        expect(y).toBeLessThanOrEqual(bottom - ROW_HEIGHT);
      });
    });
    expect(printed).toBe(300);
    expect(pages[0]?.textContent).toContain("Activity number 1");
    expect(pages.at(-1)?.textContent).toContain("Activity number 300");
  });

  /** The print view of `rows`' first `count` lines. */
  function printed(rows: ReturnType<typeof many>, count: number, onClose = () => {}) {
    return render(
      <PrintView
        rows={rows}
        lines={Int32Array.from({ length: count }, (_, k) => k)}
        bands={[]}
        columns={[{ id: "code", width: 90 }]}
        split={0.5}
        summary={SUMMARY}
        onClose={onClose}
      />,
    );
  }

  it("draws each bar as the screen does: critical red, milestones by their type, work done, and float only on work left", () => {
    const rows = many();
    // A finish milestone with only its finish; a task with no length; work done with float stored after it.
    (rows.type as string[])[1] = "finish-milestone";
    rows.early_start[1] = null;
    rows.early_finish[2] = rows.early_start[2] ?? null;
    rows.status[3] = "completed";
    rows.actual_start[3] = rows.early_start[3] ?? null;
    rows.actual_finish[3] = rows.early_finish[3] ?? null;
    printed(rows, 4);
    const [critical, milestone, zero, done] = [...document.querySelectorAll(".print-row")];
    expect(critical?.querySelector("rect")?.getAttribute("fill")).toBe("#c62828");
    expect(milestone?.querySelector("path")).toBeTruthy();
    expect(zero?.querySelector("path")).toBeNull();
    expect(zero?.querySelector("rect")).toBeTruthy();
    expect(done?.querySelector("rect")?.getAttribute("fill")).toBe("#59636e");
    // Only the row rule, no float line, on finished work.
    expect(done?.querySelectorAll("line")).toHaveLength(1);
  });

  it("spans the timescale over what's printed, not what's filtered out", () => {
    const rows = many();
    rows.early_finish[299] = "2046-01-01 17:00";
    printed(rows, 10);
    const to = screen.getByLabelText("to") as HTMLInputElement;
    expect(to.value < "2026-02-01").toBe(true);
  });

  it("says when the timescale ends before it starts, and uses the dates printed", () => {
    printed(many(), 10);
    fireEvent.change(screen.getByLabelText("Timescale from"), { target: { value: "2026-03-01" } });
    fireEvent.change(screen.getByLabelText("to"), { target: { value: "2026-02-01" } });
    expect(screen.getByRole("alert").textContent).toContain("ends before it starts");
  });

  it("closes on Escape from anywhere, keeps focus in, and gives it back", () => {
    const launcher = document.createElement("button");
    document.body.append(launcher);
    launcher.focus();
    let closed = 0;
    const { unmount } = printed(many(), 5, () => closed++);
    expect(document.body.classList.contains("printing")).toBe(true);
    expect(screen.getByRole("dialog", { name: "Print" }).contains(document.activeElement)).toBe(true);
    act(() => launcher.focus());
    expect(screen.getByRole("dialog", { name: "Print" }).contains(document.activeElement)).toBe(true);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(1);
    unmount();
    expect(document.body.classList.contains("printing")).toBe(false);
    expect(document.activeElement).toBe(launcher);
    launcher.remove();
  });

  it("prints a lone milestone with only one date, inside the Gantt, and fits a long title", () => {
    const rows = many();
    (rows.type as string[])[0] = "finish-milestone";
    rows.early_start[0] = null;
    rows.early_finish[0] = "2026-10-01 17:00";
    rows.late_finish[0] = null;
    render(
      <PrintView
        rows={rows}
        lines={Int32Array.from([0])}
        bands={[]}
        columns={[{ id: "code", width: 90 }]}
        split={0.5}
        summary={{
          ...SUMMARY,
          inspect: {
            ...SUMMARY.inspect,
            projects: SUMMARY.inspect.projects.map((p) => ({ ...p, name: "A very long project name ".repeat(12) })),
          },
        }}
        onClose={() => {}}
      />,
    );
    const row = document.querySelector(".print-row");
    expect(row?.querySelector("path")).toBeTruthy();
    // Drawn within the Gantt's clip.
    expect(row?.querySelector("path")?.closest("g[clip-path]")?.getAttribute("clip-path")).toBe("url(#print-0-gantt)");
    expect((screen.getByLabelText("to") as HTMLInputElement).value >= "2026-10-01").toBe(true);
    expect(document.querySelector(".print-title text")?.textContent).toMatch(/…$/);
  });
});
