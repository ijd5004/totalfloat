// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api, type Summary } from "../api";
import branchAnswer from "../api/golden/history-branch-1.json";
import branchesAnswer from "../api/golden/history-branches-1.json";
import { BranchPicker } from "./BranchPicker";
import { inspectOf } from "../api/testing";

afterEach(cleanup);

const SUMMARY: Summary = {
  schema: "totalfloat.summary/1",
  location: "plant.tfdb",
  branch: "main",
  head: branchesAnswer.branches[0]?.head ?? null,
  writable: true,
  calculated: true,
  inspect: inspectOf("plant.tfdb@main", []),
};

async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
}

async function picker(
  switching: (name: string) => Promise<void> = () => Promise.resolve(),
  listed: unknown = branchesAnswer,
) {
  const asked: Record<string, unknown>[] = [];
  const fetcher: typeof fetch = (_input, init) => {
    const body = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>;
    asked.push(body);
    return Promise.resolve(new Response(JSON.stringify(body.action === "list" ? listed : branchAnswer), { status: 200 }));
  };
  const onSwitch = vi.fn<(name: string) => Promise<void>>(switching);
  const onChanged = vi.fn(() => Promise.resolve());
  const said: string[] = [];
  const onMerge = vi.fn<(name: string) => void>();
  render(
    <BranchPicker
      api={new Api(null, fetcher)}
      summary={SUMMARY}
      onSwitch={onSwitch}
      onChanged={onChanged}
      say={(t) => said.push(t)}
      onMerge={onMerge}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Branch/ }));
  await settle();
  return { asked, onSwitch, onChanged, said, onMerge };
}

describe("the branch picker", () => {
  it("lists the branches, marks the one shown, and switches to another", async () => {
    const { onSwitch } = await picker();
    const list = within(screen.getByRole("list", { name: "Branches" }));
    const main = list.getByRole("button", { name: /^main/ });
    expect(main.getAttribute("aria-current")).toBe("true");
    expect((main as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(list.getByRole("button", { name: /^what-if/ }));
    await settle();
    expect(onSwitch).toHaveBeenCalledWith("what-if");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("makes a branch from what's shown and switches to it", async () => {
    const { asked, onSwitch } = await picker();
    fireEvent.change(screen.getByLabelText("New branch from what's shown"), { target: { value: "recovery" } });
    fireEvent.click(screen.getByRole("button", { name: "Create and switch" }));
    await settle();
    expect(asked.at(-1)).toEqual({ action: "create", name: "recovery", at: SUMMARY.head });
    expect(onSwitch).toHaveBeenCalledWith("recovery");
  });

  it("asks before deleting a branch, and can't delete the one shown", async () => {
    const { asked, onChanged, said } = await picker();
    expect(screen.queryByRole("button", { name: "Delete branch main" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Delete branch what-if" }));
    expect(asked.some((body) => body.action === "delete")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await settle();
    expect(asked.at(-1)).toEqual({ action: "delete", name: "what-if" });
    expect(onChanged).toHaveBeenCalled();
    expect(said.at(-1)).toContain("Deleted branch what-if");
  });

  it("takes focus as it opens, and closes on Escape, giving focus back to its button", async () => {
    await picker();
    // The shown branch can't be chosen, so focus goes to the next that can.
    expect(document.activeElement?.textContent).toContain("what-if");
    fireEvent.keyDown(document.activeElement as Element, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /Branch/ }));
  });

  it("says a branch was made when only the switch to it fails", async () => {
    const { asked, said } = await picker(() => Promise.reject(new Error("offline")));
    fireEvent.change(screen.getByLabelText("New branch from what's shown"), { target: { value: "recovery" } });
    fireEvent.click(screen.getByRole("button", { name: "Create and switch" }));
    await settle();
    expect(asked.some((body) => body.action === "create")).toBe(true);
    expect(said.at(-1)).toBe("Created recovery, but couldn't switch to it.");
    // The list is asked for again, to show it.
    expect(asked.filter((body) => body.action === "list")).toHaveLength(2);
  });

  it("lets a page pinned to a branch switch the store back to it", async () => {
    // The page shows main, but the store has what-if checked out.
    const pinned = structuredClone(branchesAnswer);
    for (const b of pinned.branches) b.current = b.name === "what-if";
    const { onSwitch } = await picker(undefined, pinned);
    const list = within(screen.getByRole("list", { name: "Branches" }));
    const main = list.getByRole("button", { name: /^main/ });
    expect((main as HTMLButtonElement).disabled).toBe(false);
    expect(list.getByRole("button", { name: /^what-if/ }).textContent).toContain("(checked out)");
    // A merge records on the checked-out branch, so none is offered into the one pinned.
    expect(screen.queryByRole("button", { name: /^Merge / })).toBeNull();
    // The checked-out branch can't be deleted.
    expect(screen.queryByRole("button", { name: "Delete branch what-if" })).toBeNull();
    fireEvent.click(main);
    await settle();
    expect(onSwitch).toHaveBeenCalledWith("main");
  });

  it("offers to merge each other branch into the one shown", async () => {
    const { onMerge } = await picker();
    expect(screen.queryByRole("button", { name: "Merge main into main" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Merge what-if into main" }));
    expect(onMerge).toHaveBeenCalledWith("what-if");
    expect(screen.queryByRole("dialog", { name: "Branches" })).toBeNull();
  });
});
