// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { Registry, appTakes } from "./commands";

const press = (key: string, mods: { ctrlKey?: boolean; altKey?: boolean } = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("keys typed in a text field", () => {
  const registry = new Registry();
  registry.register({ id: "palette.open", name: "Palette", group: "G", keys: ["mod+k"], run: () => {} });
  registry.register({ id: "edit.undo", name: "Undo", group: "G", keys: ["mod+z"], inText: false, run: () => {} });
  registry.register({ id: "help.keys", name: "Keys", group: "G", keys: ["?"], run: () => {} });
  const field = document.createElement("input");
  const grid = document.createElement("div");

  it("stay the field's, unless they carry Ctrl, Cmd or Alt", () => {
    expect(appTakes(registry, press("?"), field)).toBe(false);
    expect(appTakes(registry, press("k", { ctrlKey: true }), field)).toBe(true);
  });

  it("leave keys pressed in the menu to the menu", () => {
    const bar = document.createElement("div");
    bar.setAttribute("role", "menu");
    const item = document.createElement("button");
    bar.append(item);
    expect(appTakes(registry, press("?"), item)).toBe(false);
    expect(appTakes(registry, press("k", { ctrlKey: true }), item)).toBe(true);
  });

  it("leave the field its own undo", () => {
    expect(appTakes(registry, press("z", { ctrlKey: true }), field)).toBe(false);
    expect(appTakes(registry, press("z", { ctrlKey: true }), grid)).toBe(true);
  });
});
