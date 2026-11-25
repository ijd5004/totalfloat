// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Registry } from "../commands";
import type { ZoomChoice } from "../gantt/time";
import { Toolbar } from "./Toolbar";

afterEach(cleanup);

function registry() {
  const ran: string[] = [];
  const keys = new Registry();
  keys.register({ id: "gantt.lines", name: "Logic lines: all, selected, none", label: "Logic lines", group: "View", keys: ["alt+l"], run: () => ran.push("lines") });
  keys.register({ id: "gantt.driving", name: "Driving path of the selection", label: "Driving path", group: "Schedule", keys: ["alt+shift+d"], run: () => ran.push("driving") });
  keys.register({ id: "view.panel", name: "Right column of the schedule", label: "Right column", group: "View", keys: ["mod+\\"], run: () => ran.push("panel") });
  keys.register({ id: "layout.save-as", name: "Save layout as…", group: "View", keys: [], run: () => ran.push("save-as") });
  return { keys, ran };
}

function toolbar(zoom: ZoomChoice, onZoom: (zoom: ZoomChoice) => void = () => {}) {
  const { keys, ran } = registry();
  render(
    <Toolbar
      registry={keys}
      mac={false}
      layout="Look ahead"
      zoom={zoom}
      onZoom={onZoom}
      toggles={[
        { command: "gantt.lines", icon: "link", pressed: true },
        { command: "gantt.driving", icon: "drive", pressed: false },
        { command: "view.panel", icon: "panel", pressed: true },
      ]}
      onSlot={() => {}}
    />,
  );
  return ran;
}

describe("the toolbar", () => {
  it("marks the zoom shown, and sets the one chosen", () => {
    const chosen: ZoomChoice[] = [];
    toolbar("fit", (zoom) => chosen.push(zoom));
    const zoom = screen.getByRole("radiogroup", { name: "Zoom" });
    const radios = within(zoom).getAllByRole("radio");
    expect(radios.map((r) => r.textContent)).toEqual(["Week", "Month", "Quarter", "Fit"]);
    expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["false", "false", "false", "true"]);
    for (const radio of radios) fireEvent.click(radio);
    expect(chosen).toEqual([3, 2, 0, "fit"]);
    cleanup();
    // A scale with no name of its own marks none.
    toolbar(5);
    expect(screen.getAllByRole("radio").every((r) => r.getAttribute("aria-checked") === "false")).toBe(true);
  });

  it("names each toggle, with its key in the tooltip, and runs its command", () => {
    const ran = toolbar("fit");
    const lines = screen.getByRole("button", { name: "Logic lines" });
    expect(lines.getAttribute("title")).toBe("Logic lines (Alt+L)");
    expect(lines.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Driving path" }).getAttribute("title")).toBe("Driving path (Alt+Shift+D)");
    expect(screen.getByRole("button", { name: "Driving path" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "Right column" }).getAttribute("title")).toBe("Right column (Ctrl+\\)");
    fireEvent.click(lines);
    fireEvent.click(screen.getByRole("button", { name: "Driving path" }));
    expect(ran).toEqual(["lines", "driving"]);
  });

  it("names the layout open, and runs a layout command from its menu", () => {
    const ran = toolbar("fit");
    const picker = screen.getByRole("button", { name: "Layout: Look ahead" });
    expect(picker.textContent).toContain("Look ahead");
    fireEvent.click(picker);
    fireEvent.click(screen.getByRole("menuitem", { name: "Save layout as…" }));
    expect(ran).toEqual(["save-as"]);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("hides Add activity until it's given something to do", () => {
    toolbar("fit");
    expect(screen.queryByRole("button", { name: "Add activity" })).toBeNull();
  });
});
