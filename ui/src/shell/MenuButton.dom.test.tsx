// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Registry } from "../commands";
import { MenuButton, SECTIONS, sectionsOf } from "./MenuButton";

afterEach(cleanup);

function registry() {
  const ran: string[] = [];
  const keys = new Registry();
  keys.register({ id: "file.export", name: "Export as XER…", label: "Export XER", group: "File", keys: ["alt+shift+x"], run: () => ran.push("export") });
  keys.register({ id: "data.reload", name: "Reload schedule", label: "Reload", group: "File", keys: ["alt+r"], run: () => ran.push("reload") });
  keys.register({ id: "edit.undo", name: "Undo last change", label: "Undo", group: "Edit", keys: ["mod+z"], enabled: () => false, run: () => ran.push("undo") });
  keys.register({ id: "view.panel", name: "Right column of the schedule", label: "Right column", group: "View", keys: ["mod+\\"], run: () => ran.push("panel") });
  keys.register({ id: "explain.open", name: "Explain the selection's dates", label: "Explain", group: "Schedule", keys: ["alt+x"], run: () => ran.push("explain") });
  keys.register({ id: "help.keys", name: "Keyboard shortcuts", group: "Help", keys: ["?"], run: () => ran.push("keys") });
  return { keys, ran };
}

/** Alt+Shift+M, as the app's key handler gives it to the registry; says whether an action ran. */
function menuKey(keys: Registry): boolean {
  let ran = false;
  act(() => {
    ran = keys.press({ key: "M", code: "KeyM", altKey: true, shiftKey: true, ctrlKey: false, metaKey: false });
  });
  return ran;
}

/** Every item the open menu lists, as its text. */
const listed = () => screen.getAllByRole("menuitem").map((item) => item.textContent);

describe("the Menu button", () => {
  it("lists each registered action once, in its section, with its short label and keys", () => {
    const { keys } = registry();
    expect([...sectionsOf(keys).keys()]).toEqual([...SECTIONS]);
    render(<MenuButton registry={keys} mac={false} />);
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    const menu = screen.getByRole("menu", { name: "Menu" });
    expect(within(menu).getAllByRole("group").map((g) => g.getAttribute("aria-label"))).toEqual([...SECTIONS]);
    expect(listed()).toEqual(["Export XERAlt+Shift+X", "ReloadAlt+R", "UndoCtrl+Z", "Right columnCtrl+\\", "ExplainAlt+X", "Keyboard shortcuts?"]);
    expect(within(within(menu).getByRole("group", { name: "File" })).getAllByRole("menuitem")).toHaveLength(2);
    // The full name is the item's tooltip.
    expect(screen.getByRole("menuitem", { name: /Export XER/ }).getAttribute("title")).toBe("Export as XER…");
  });

  it("marks what can't run now, and doesn't run it", () => {
    const { keys, ran } = registry();
    render(<MenuButton registry={keys} mac={false} />);
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    const undo = screen.getByRole("menuitem", { name: /Undo/ });
    expect(undo.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByRole("menuitem", { name: /Reload/ }).getAttribute("aria-disabled")).toBeNull();
    fireEvent.click(undo);
    expect(ran).toEqual([]);
    expect(screen.getByRole("menu")).toBeTruthy();
  });

  it("runs an item by click, and closes", () => {
    const { keys, ran } = registry();
    render(<MenuButton registry={keys} mac={false} />);
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Reload/ }));
    expect(ran).toEqual(["reload"]);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("works from the keyboard: Alt+Shift+M opens it, arrows pass over nothing, Enter runs, Escape gives focus back", () => {
    const { keys, ran } = registry();
    render(<MenuButton registry={keys} mac={false} />);
    menuKey(keys);
    expect(document.activeElement?.textContent).toBe("Export XERAlt+Shift+X");
    fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
    fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
    // Undo can't run, but can be reached, as WAI-ARIA asks.
    expect(document.activeElement?.textContent).toBe("UndoCtrl+Z");
    fireEvent.keyDown(document.activeElement as Element, { key: "End" });
    expect(document.activeElement?.textContent).toBe("Keyboard shortcuts?");
    fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Export XERAlt+Shift+X");
    // Type-ahead: E goes to the next item starting with it.
    fireEvent.keyDown(document.activeElement as Element, { key: "e" });
    expect(document.activeElement?.textContent).toBe("ExplainAlt+X");
    fireEvent.keyDown(document.activeElement as Element, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Menu" }));
    fireEvent.keyDown(document.activeElement as Element, { key: "ArrowUp" });
    expect(document.activeElement?.textContent).toBe("Keyboard shortcuts?");
    fireEvent.click(document.activeElement as Element);
    expect(ran).toEqual(["keys"]);
  });

  it("opens at the first item that can run", () => {
    const keys = new Registry();
    keys.register({ id: "edit.undo", name: "Undo last change", label: "Undo", group: "Edit", keys: ["mod+z"], enabled: () => false, run: () => {} });
    keys.register({ id: "edit.preview", name: "Preview edits before recording", label: "Preview edits", group: "Edit", keys: [], run: () => {} });
    render(<MenuButton registry={keys} mac={false} />);
    const button = screen.getByRole("button", { name: "Menu" });
    button.focus();
    fireEvent.keyDown(button, { key: "ArrowDown" });
    // Undo can't run, so Preview edits takes focus.
    expect(document.activeElement?.textContent).toBe("Preview edits");
  });

  it("is in the key list as Alt+Shift+M, and leaves F10 to the browser", () => {
    const { keys } = registry();
    render(<MenuButton registry={keys} mac={false} />);
    expect(keys.forKey("alt+shift+m")?.id).toBe("menu.open");
    expect(keys.forKey("alt+shift+m")?.group).toBe("Menu");
    fireEvent.keyDown(window, { key: "F10" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("leaves Alt+Shift+M and Alt to a field being typed in", () => {
    const { keys } = registry();
    render(
      <div>
        <MenuButton registry={keys} mac={false} />
        <input aria-label="Cell" />
      </div>,
    );
    const cell = screen.getByRole("textbox", { name: "Cell" });
    cell.focus();
    expect(menuKey(keys)).toBe(false);
    fireEvent.keyDown(window, { key: "Alt" });
    fireEvent.keyUp(window, { key: "Alt" });
    expect(document.activeElement).toBe(cell);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes when focus goes elsewhere, as when the palette opens, and on a click elsewhere", () => {
    const { keys } = registry();
    render(
      <div>
        <MenuButton registry={keys} mac={false} />
        <input aria-label="Search commands" />
        <p>Elsewhere</p>
      </div>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.focusIn(screen.getByRole("textbox", { name: "Search commands" }));
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Menu" }));
    fireEvent.mouseDown(screen.getByText("Elsewhere"));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("gives focus back to the work after a mouse-run action, after Alt+Shift+M twice, and after Escape twice", () => {
    const { keys } = registry();
    render(
      <div>
        <MenuButton registry={keys} mac={false} />
        <div className="grid-body" tabIndex={0} aria-label="Grid" />
      </div>,
    );
    const grid = screen.getByLabelText("Grid");
    grid.focus();
    const button = screen.getByRole("button", { name: "Menu" });
    fireEvent.mouseDown(button);
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("menuitem", { name: /Right column/ }));
    expect(document.activeElement).toBe(grid);
    // Alt+Shift+M opens it and Alt+Shift+M again closes it, back to the grid.
    menuKey(keys);
    expect(screen.getByRole("menu")).toBeTruthy();
    menuKey(keys);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(grid);
    // Escape closes it to the button, and Escape again goes back to the grid.
    menuKey(keys);
    fireEvent.keyDown(document.activeElement as Element, { key: "Escape" });
    expect(document.activeElement).toBe(button);
    fireEvent.keyDown(document.activeElement as Element, { key: "Escape" });
    expect(document.activeElement).toBe(grid);
  });

  it("goes back to the grid on Escape from the button, though focus came by Tab", () => {
    const { keys } = registry();
    render(
      <div>
        <MenuButton registry={keys} mac={false} />
        <div className="grid-body" tabIndex={0} aria-label="Grid" />
      </div>,
    );
    const button = screen.getByRole("button", { name: "Menu" });
    button.focus();
    fireEvent.keyDown(button, { key: "Escape" });
    expect(document.activeElement).toBe(screen.getByLabelText("Grid"));
  });

  it("keeps the keyboard's item when the menu opens under a resting pointer", () => {
    const { keys } = registry();
    render(<MenuButton registry={keys} mac={false} />);
    fireEvent.keyDown(screen.getByRole("button", { name: "Menu" }), { key: "ArrowDown" });
    const reload = screen.getByRole("menuitem", { name: /Reload/ });
    fireEvent.mouseEnter(reload);
    expect(document.activeElement?.textContent).toBe("Export XERAlt+Shift+X");
    fireEvent.mouseMove(reload);
    expect(document.activeElement?.textContent).toBe("ReloadAlt+R");
  });
});
