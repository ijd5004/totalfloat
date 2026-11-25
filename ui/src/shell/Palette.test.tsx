// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Registry } from "../commands";
import { KeyList } from "./KeyList";
import { Palette } from "./Palette";

afterEach(cleanup);

function registry(ran: string[]): Registry {
  const registry = new Registry();
  registry.register({ id: "a", name: "Jump to activity", group: "Grid", keys: ["mod+j"], run: () => ran.push("a") });
  registry.register({ id: "b", name: "Reload the schedule", group: "General", keys: [], run: () => ran.push("b") });
  return registry;
}

describe("the command palette", () => {
  it("lists an action registered with a key, with its key", () => {
    render(<Palette registry={registry([])} mac={false} onClose={() => {}} />);
    const option = screen.getByRole("option", { name: /Jump to activity/ });
    expect(option.textContent).toContain("Ctrl+J");
  });

  it("filters as you type and runs the chosen action on Enter", () => {
    const ran: string[] = [];
    let closed = false;
    render(<Palette registry={registry(ran)} mac={true} onClose={() => (closed = true)} />);
    const input = screen.getByRole("textbox", { name: "Command" });
    fireEvent.change(input, { target: { value: "reload" } });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(ran).toEqual(["b"]);
    expect(closed).toBe(true);
  });
});

describe("the key list", () => {
  it("lists only actions with keys, by group", () => {
    render(<KeyList registry={registry([])} mac={true} onClose={() => {}} />);
    expect(screen.getByText("Jump to activity")).toBeTruthy();
    expect(screen.queryByText("Reload the schedule")).toBeNull();
    expect(screen.getByText("⌘J")).toBeTruthy();
  });
});
