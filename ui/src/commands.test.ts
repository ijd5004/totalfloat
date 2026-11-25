// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { createElement } from "react";
import { afterEach, describe, expect, it } from "vitest";

import { Api } from "./api";
import { inspectOf } from "./api/testing";
import { Registry, bindingOf, describeKey } from "./commands";
import { sample } from "./edit/fixture";
import { Shell } from "./shell/Shell";

afterEach(cleanup);

const press = (
  key: string,
  mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {},
) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("bindings", () => {
  it("read Ctrl and Cmd alike as mod", () => {
    expect(bindingOf(press("k", { ctrlKey: true }))).toBe("mod+k");
    expect(bindingOf(press("K", { metaKey: true, shiftKey: true }))).toBe("mod+shift+k");
  });

  it("read a modified letter from its physical key, as Option on a Mac changes the character", () => {
    expect(bindingOf({ ...press("®", { altKey: true }), code: "KeyR" })).toBe("alt+r");
    expect(bindingOf({ ...press("k", { ctrlKey: true }), code: "KeyK" })).toBe("mod+k");
  });

  it("read a modified = or - from its physical key, as Option on a Mac types ≠ and –", () => {
    expect(bindingOf({ ...press("≠", { altKey: true }), code: "Equal" })).toBe("alt+=");
    expect(bindingOf({ ...press("–", { altKey: true }), code: "Minus" })).toBe("alt+-");
    expect(bindingOf({ ...press("=", { altKey: true }), code: "Equal" })).toBe("alt+=");
  });

  it("keep the layout's letter when it gives one, on remapped layouts too", () => {
    expect(bindingOf({ ...press("k", { ctrlKey: true }), code: "KeyV" })).toBe("mod+k");
    expect(bindingOf({ ...press("t", { ctrlKey: true }), code: "KeyK" })).toBe("mod+t");
  });

  it("take a typed character as itself", () => {
    expect(bindingOf(press("?", { shiftKey: true }))).toBe("?");
    expect(bindingOf(press("Escape"))).toBe("escape");
  });

  it("show the plus key as itself", () => {
    expect(describeKey("+", false)).toBe("+");
    expect(describeKey("+", true)).toBe("+");
  });

  it("read as people write them", () => {
    expect(describeKey("mod+k", false)).toBe("Ctrl+K");
    expect(describeKey("mod+k", true)).toBe("⌘K");
    expect(describeKey("escape", false)).toBe("Esc");
  });
});

describe("the registry", () => {
  it("runs the action bound to a key, once registered, and not after", () => {
    const registry = new Registry();
    let ran = 0;
    const remove = registry.register({ id: "a", name: "Do A", group: "Test", keys: ["mod+j"], run: () => ran++ });
    expect(registry.press(press("j", { ctrlKey: true }))).toBe(true);
    remove();
    expect(registry.press(press("j", { ctrlKey: true }))).toBe(false);
    expect(ran).toBe(1);
  });

  it("refuses one key for two actions", () => {
    const registry = new Registry();
    registry.register({ id: "a", name: "A", group: "T", keys: ["?"], run: () => {} });
    expect(() => registry.register({ id: "b", name: "B", group: "T", keys: ["?"], run: () => {} })).toThrow(/both/);
  });

  it("finds actions by every word typed, leaving out those that can't run", () => {
    const registry = new Registry();
    registry.register({ id: "a", name: "Show keyboard shortcuts", group: "Help", keys: [], run: () => {} });
    registry.register({ id: "b", name: "Show side panel", group: "View", keys: [], run: () => {}, enabled: () => false });
    expect(registry.search("show key").map((a) => a.id)).toEqual(["a"]);
    expect(registry.search("side")).toEqual([]);
  });
});

/** Whether a binding is a character the grid would type into a cell: one with no Ctrl, Cmd or Alt. */
const typed = (key: string) => !/^(?:mod|alt)\+/.test(key) && [...key].length === 1;

/** The registry of the whole app, as the shell fills it on a small schedule. */
async function appRegistry(): Promise<Registry> {
  const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
  const fetcher: typeof fetch = (input) => {
    const url = String(input);
    if (url.startsWith("/api/1/summary")) {
      return Promise.resolve(
        json({ location: "plant.tfdb", branch: "main", head: "h1", writable: true, calculated: true, inspect: inspectOf("plant.tfdb@main", []) }),
      );
    }
    if (url.startsWith("/api/1/rows")) return Promise.resolve(json(sample()));
    if (url.startsWith("/api/1/wbs")) return Promise.resolve(json({ schema: "totalfloat.wbs/1", projects: [] }));
    if (url.startsWith("/api/1/fields")) return Promise.resolve(json({ schema: "totalfloat.fields/1", activity_codes: [], activity_udfs: [] }));
    return Promise.resolve(json({ count: 0, id: [], predecessor: [], successor: [], type: [], lag_hours: [], driving: [] }));
  };
  const registry = new Registry();
  render(createElement(Shell, { api: new Api(null, fetcher), registry, follow: false }));
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
  return registry;
}

/**
 * Keys Microsoft Edge and Google Chrome on Windows keep for themselves, so a
 * page never sees them: from Microsoft's "Keyboard shortcuts in
 * Microsoft Edge" and Google's "Chrome keyboard shortcuts" (Windows and
 * Linux), as published in November 2026, and Alt+Shift+S and Ctrl+Shift+S,
 * which Edge takes for Visual Search and Web capture though its list leaves
 * them out.
 */
const BROWSERS_KEEP = [
  "alt+d", "alt+e", "alt+f",
  "alt+shift+a", "alt+shift+b", "alt+shift+i", "alt+shift+n", "alt+shift+s", "alt+shift+t",
  ...[..."bdegijklmnoprstuvwy"].map((letter) => `mod+shift+${letter}`),
  "f10",
];

describe("the app's keys", () => {
  it("leave every character to the grid, bar / and ?", async () => {
    const registry = await appRegistry();
    const actions = registry.inOrder();
    expect(actions.length).toBeGreaterThan(20);
    const bare = actions.flatMap((action) => action.keys.filter(typed).map((key) => `${action.id}: ${key}`));
    expect(bare.sort()).toEqual(["grid.filter: /", "help.keys: ?"]);
  });

  it("leave the keys Edge and Chrome keep to the browser", async () => {
    const registry = await appRegistry();
    const kept = registry.inOrder().flatMap((action) => action.keys.filter((key) => BROWSERS_KEEP.includes(key)).map((key) => `${action.id}: ${key}`));
    expect(kept).toEqual([]);
    // The keys that replaced them.
    expect(registry.forKey("alt+shift+g")?.id).toBe("view.schedule");
    expect(registry.forKey("alt+shift+w")?.id).toBe("view.project-network");
    expect(registry.forKey("alt+shift+x")?.id).toBe("file.export");
    expect(registry.forKey("alt+shift+m")?.id).toBe("menu.open");
  });

  it("are read as characters only without Ctrl, Cmd or Alt", () => {
    expect(typed("e")).toBe(true);
    expect(typed("+")).toBe(true);
    expect(typed("alt+x")).toBe(false);
    expect(typed("alt+-")).toBe(false);
    expect(typed("escape")).toBe(false);
  });
});
