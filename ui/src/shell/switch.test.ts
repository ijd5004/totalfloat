import { describe, expect, it } from "vitest";

import { Api, type Summary } from "../api";
import { sample } from "../edit/fixture";
import { Editing } from "../edit/editing";
import { switchBranch } from "./switch";
import { inspectOf } from "../api/testing";

const MAIN: Summary = {
  schema: "totalfloat.summary/1",
  location: "plant.tfdb",
  branch: "main",
  head: "h1",
  writable: true,
  calculated: true,
  inspect: inspectOf("plant.tfdb@main", []),
};

/** A server whose edits never answer, so an edit sent stays in flight; branch requests are noted. */
function server() {
  const switched: string[] = [];
  const fetcher: typeof fetch = (input, init) => {
    const url = String(input);
    if (url.startsWith("/api/1/edit")) return new Promise(() => {});
    const body = JSON.parse(String(init?.body ?? "{}")) as { name?: string };
    switched.push(body.name ?? "");
    return Promise.resolve(new Response(JSON.stringify({ schema: "totalfloat.history-switch/1" }), { status: 200 }));
  };
  return { api: new Api(null, fetcher), switched };
}

function editing(api: Api) {
  const storage = new Map<string, string>();
  const store = {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => void storage.set(k, v),
  } as unknown as Storage;
  const editor = new Editing(api, store);
  editor.show({ rows: sample(), head: "h1", writable: true });
  return editor;
}

describe("switching branch", () => {
  it("waits for an edit being recorded, so it can't land on the branch switched to", async () => {
    const { api, switched } = server();
    const editor = editing(api);
    void editor.commit([{ id: 1, column: "name", text: "Pour" }]);
    expect(editor.state().busy).toBe(true);
    await expect(switchBranch(api, editor, "what-if", () => Promise.resolve({ summary: MAIN }), () => {})).rejects.toMatchObject({
      code: "edit-in-progress",
    });
    expect(switched).toEqual([]);
  });

  it("drops typed values not yet sent, and says when the page is pinned to another branch", async () => {
    const { api, switched } = server();
    const editor = editing(api);
    editor.setPreviewing(true);
    const said: string[] = [];
    // A value refused before sending is kept as typed, and isn't busy.
    await editor.commit([{ id: 1, column: "original", text: "5x" }]);
    expect(editor.state().marks.size).toBe(1);
    editor.setMessage("typed on main");
    await switchBranch(api, editor, "what-if", () => Promise.resolve({ summary: MAIN }), (text) => said.push(text));
    expect(switched).toEqual(["what-if"]);
    expect(editor.state().marks.size).toBe(0);
    expect(editor.state().pending).toBeNull();
    // Nor is its message kept for the branch switched to.
    expect(editor.state().message).toBe("");
    expect(said.at(-1)).toBe("The store is on what-if now, but this page serves plant.tfdb, so it still shows main.");
    await switchBranch(api, editor, "what-if", () => Promise.resolve({ summary: { ...MAIN, branch: "what-if" } }), (text) =>
      said.push(text),
    );
    expect(said.at(-1)).toBe("Switched to what-if.");
  });

  it("holds edits until the branch switched to is shown", async () => {
    let answerSwitch: () => void = () => {};
    const posted: string[] = [];
    const fetcher: typeof fetch = (input) => {
      const url = String(input);
      posted.push(url);
      if (url.startsWith("/api/1/history/branch")) {
        return new Promise((resolve) => {
          answerSwitch = () => resolve(new Response(JSON.stringify({}), { status: 200 }));
        });
      }
      return new Promise(() => {});
    };
    const api = new Api(null, fetcher);
    const editor = editing(api);
    editor.setPreviewing(false);
    const switching = switchBranch(api, editor, "what-if", () => Promise.resolve({ summary: MAIN }), () => {});
    // Typed while the switch is out: refused, and nothing is sent.
    await editor.commit([{ id: 1, column: "name", text: "Pour" }]);
    expect(posted.filter((url) => url.startsWith("/api/1/edit"))).toEqual([]);
    expect(editor.state().notice?.text).toBe("The branch is being switched; edit once it's shown.");
    answerSwitch();
    await switching;
    // Once it's shown, edits go again.
    void editor.commit([{ id: 1, column: "name", text: "Pour" }]);
    expect(posted.filter((url) => url.startsWith("/api/1/edit"))).toHaveLength(1);
  });
});
