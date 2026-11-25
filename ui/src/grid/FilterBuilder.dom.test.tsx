// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { ApiError } from "../api";
import { FilterBuilder, type SavedFilters, said } from "./FilterBuilder";
import type { Built, Condition } from "./filters";

afterEach(cleanup);

/** Named filters kept in memory, as the server keeps them in files; `start` gives each one's `built`. */
function shelf(start: Record<string, Record<string, string>> = {}): SavedFilters & { kept: Map<string, Built> } {
  const kept = new Map<string, Built>(Object.entries(start).map(([name, built]) => [name, { built, any: [] }]));
  return {
    kept,
    list: () => Promise.resolve([...kept.keys()].sort()),
    read: (name) => {
      const found = kept.get(name);
      return found ? Promise.resolve(found) : Promise.reject(new Error(`no filter ${name}`));
    },
    save: (name, filter) => {
      if (name.includes("/")) {
        return Promise.reject(new ApiError(400, "bad-filter-name", `"${name}" can't name a filter`, "use letters"));
      }
      kept.set(name, filter);
      return Promise.resolve();
    },
    remove: (name) => {
      kept.delete(name);
      return Promise.resolve();
    },
  };
}

async function settle() {
  await act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
}

function builder(saved: SavedFilters, applied: Condition[] = [], onApply: (conditions: Condition[]) => Promise<string | null> = () => Promise.resolve(null)) {
  render(
    <FilterBuilder
      applied={applied}
      codes={[{ type: "Phase", values: ["DESIGN", "BUILD"] }]}
      saved={saved}
      onApply={onApply}
      onClear={() => {}}
      onClose={() => {}}
    />,
  );
}

describe("named filters in the builder", () => {
  it("saves the conditions shown under a name, as query's parameters, and lists it", async () => {
    const saved = shelf();
    builder(saved, [
      { field: "code", value: "Phase=DESIGN" },
      { field: "critical", value: "" },
      { field: "start_from", value: "2026-01-05" },
    ]);
    await settle();
    expect(screen.getByText("No saved filters yet.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Filter name"), { target: { value: "Critical design" } });
    fireEvent.click(screen.getByText("Save as"));
    await settle();
    expect(saved.kept.get("Critical design")).toEqual({
      built: { code: "Phase=DESIGN", critical: "true", start_from: "2026-01-05 00:00" },
      any: [],
    });
    expect((screen.getByLabelText("Saved filter") as HTMLSelectElement).value).toBe("Critical design");
    expect(screen.getByRole("status").textContent).toBe("Saved filter Critical design.");
  });

  it("applies a saved filter: its conditions shown and applied", async () => {
    const saved = shelf({ "Late work": { float_max: "0", status: "not-started" } });
    const applied: Condition[][] = [];
    builder(saved, [], (conditions) => {
      applied.push(conditions);
      return Promise.resolve(null);
    });
    await settle();
    fireEvent.click(screen.getByText("Apply saved"));
    await settle();
    expect(applied).toEqual([
      [
        { field: "status", value: "not-started" },
        { field: "float_max", value: "0" },
      ],
    ]);
    expect((screen.getByLabelText("Total float at most (h) value") as HTMLInputElement).value).toBe("0");
  });

  it("deletes one, and says why a name is refused", async () => {
    const saved = shelf({ A: { critical: "true" }, B: { critical: "true" } });
    builder(saved, [{ field: "critical", value: "" }]);
    await settle();
    fireEvent.click(screen.getByText("Delete…"));
    // Nothing goes until it's confirmed; Keep backs out.
    fireEvent.click(screen.getByText("Keep"));
    expect(saved.kept.has("A")).toBe(true);
    fireEvent.click(screen.getByText("Delete…"));
    fireEvent.click(screen.getByLabelText("Delete saved filter A"));
    await settle();
    expect([...saved.kept.keys()]).toEqual(["B"]);
    expect((screen.getByLabelText("Saved filter") as HTMLSelectElement).value).toBe("B");
    fireEvent.change(screen.getByLabelText("Filter name"), { target: { value: "a/b" } });
    fireEvent.click(screen.getByText("Save as"));
    await settle();
    expect(screen.getByRole("alert").textContent).toContain("can't name a filter");
    expect(saved.kept.has("a/b")).toBe(false);
  });

  it("doesn't apply a saved filter read after Clear", async () => {
    let answer: (filter: Built) => void = () => {};
    const saved = { ...shelf({ Late: { critical: "true" } }), read: () => new Promise<Built>((done) => (answer = done)) };
    const applied: Condition[][] = [];
    builder(saved, [], (conditions) => {
      applied.push(conditions);
      return Promise.resolve(null);
    });
    await settle();
    fireEvent.click(screen.getByText("Apply saved"));
    fireEvent.click(screen.getByText("Clear"));
    answer({ built: { critical: "true" }, any: [] });
    await settle();
    expect(applied).toEqual([]);
  });

  it("won't save under no name, even with Enter", async () => {
    const saved = shelf();
    builder(saved, [{ field: "critical", value: "" }]);
    await settle();
    fireEvent.keyDown(screen.getByLabelText("Filter name"), { key: "Enter" });
    await settle();
    expect(saved.kept.size).toBe(0);
  });

  it("won't save an empty or unfinished filter", async () => {
    const saved = shelf();
    builder(saved, [{ field: "float_max", value: "" }]);
    await settle();
    fireEvent.change(screen.getByLabelText("Filter name"), { target: { value: "Nothing" } });
    fireEvent.click(screen.getByText("Save as"));
    await settle();
    expect(saved.kept.size).toBe(0);
    expect(screen.getByRole("alert").textContent).toContain("Give each condition a value");
  });
});

describe("any-of groups in the builder", () => {
  it("builds two groups, saves them as any, and applies them back as they were", async () => {
    const saved = shelf();
    const applied: Condition[][] = [];
    builder(saved, [{ field: "status", value: "not-started" }], (conditions) => {
      applied.push(conditions);
      return Promise.resolve(null);
    });
    await settle();
    fireEvent.click(screen.getByText("Add an “any of” group"));
    fireEvent.change(screen.getByLabelText("Add a condition, group 1"), { target: { value: "critical" } });
    fireEvent.click(screen.getByText("Add an “any of” group"));
    fireEvent.change(screen.getByLabelText("Add a condition, group 2"), { target: { value: "float_max" } });
    fireEvent.change(screen.getByLabelText("Total float at most (h), group 2 value"), { target: { value: "8" } });
    // Each group offers every filter again, apart from its own.
    fireEvent.change(screen.getByLabelText("Add a condition, group 2"), { target: { value: "critical" } });
    fireEvent.click(screen.getByText("Apply"));
    await settle();
    expect(applied).toEqual([
      [
        { field: "status", value: "not-started" },
        { field: "critical", value: "", group: 0 },
        { field: "float_max", value: "8", group: 1 },
        { field: "critical", value: "", group: 1 },
      ],
    ]);
    fireEvent.change(screen.getByLabelText("Filter name"), { target: { value: "Either" } });
    fireEvent.click(screen.getByText("Save as"));
    await settle();
    expect(saved.kept.get("Either")).toEqual({
      built: { status: "not-started" },
      any: [{ critical: "true" }, { float_max: "8", critical: "true" }],
    });
    // Cleared, then the saved one brings both groups back.
    fireEvent.click(screen.getByText("Clear"));
    fireEvent.click(screen.getByText("Apply saved"));
    await settle();
    expect(applied.at(-1)).toEqual([
      { field: "status", value: "not-started" },
      { field: "critical", value: "", group: 0 },
      { field: "float_max", value: "8", group: 1 },
      { field: "critical", value: "", group: 1 },
    ]);
    expect(screen.getByLabelText("Any of, group 2")).toBeTruthy();
  });

  it("won't apply a group with no condition, and a removed group goes with its conditions", async () => {
    const applied: Condition[][] = [];
    builder(shelf(), [{ field: "critical", value: "", group: 0 }], (conditions) => {
      applied.push(conditions);
      return Promise.resolve(null);
    });
    await settle();
    fireEvent.click(screen.getByText("Add an “any of” group"));
    fireEvent.click(screen.getByText("Apply"));
    await settle();
    expect(applied).toEqual([]);
    expect(screen.getByRole("alert").textContent).toContain("Give each “any of” group a condition");
    fireEvent.click(screen.getByLabelText("Remove group 2"));
    fireEvent.click(screen.getByLabelText("Remove group 1"));
    fireEvent.click(screen.getByText("Apply"));
    await settle();
    expect(applied).toEqual([[]]);
  });
});

describe("a refusal from the server", () => {
  it("says its message and hint as two sentences", () => {
    const refused = new ApiError(400, "no-such-wbs", "no WBS node has the code or path P.9", "give a node's code, or its path from the top as the WBS path column shows it");
    expect(said(refused)).toBe("no WBS node has the code or path P.9. Give a node's code, or its path from the top as the WBS path column shows it");
    expect(said(new ApiError(400, "x", "It stopped.", "try again"))).toBe("It stopped. Try again");
    expect(said(new ApiError(400, "x", "It stopped", ""))).toBe("It stopped");
  });
});
