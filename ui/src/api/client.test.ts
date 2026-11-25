import { describe, expect, it } from "vitest";

import { Api, ApiError, SESSION_HEADER } from "./client";
import { STORAGE_KEY, takeSession } from "./session";

function memory(): Pick<Storage, "getItem" | "setItem"> & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
  };
}

describe("takeSession", () => {
  it("keeps a token from the fragment and clears the fragment", () => {
    const storage = memory();
    let forgotten = false;
    const token = takeSession({ hash: "#session=abc" }, storage, () => {
      forgotten = true;
    });
    expect(token).toBe("abc");
    expect(forgotten).toBe(true);
    expect(storage.items.get(STORAGE_KEY)).toBe("abc");
  });

  it("uses the kept token when the fragment has none", () => {
    const storage = memory();
    expect(takeSession({ hash: "" }, storage, () => {})).toBeNull();
    storage.setItem(STORAGE_KEY, "kept");
    expect(takeSession({ hash: "" }, storage, () => {})).toBe("kept");
  });
});

describe("Api", () => {
  it("sends the session header and reads the summary", async () => {
    let sent: Headers | undefined;
    const fetcher: typeof fetch = (_input, init) => {
      sent = new Headers(init?.headers);
      return Promise.resolve(
        new Response(JSON.stringify({ inspect: { projects: [{ name: "Plant" }] } }), { status: 200 }),
      );
    };
    const summary = await new Api("tok", fetcher).summary();
    expect(sent?.get(SESSION_HEADER)).toBe("tok");
    expect(summary.inspect.projects[0]?.name).toBe("Plant");
  });

  it("turns a refusal into an ApiError with its code and hint", async () => {
    const fetcher: typeof fetch = () =>
      Promise.resolve(
        new Response(
          JSON.stringify({
            schema: "totalfloat.error/1",
            error: { code: "not-signed-in", message: "No session.", hint: "Open the link." },
          }),
          { status: 401 },
        ),
      );
    const refused = await new Api(null, fetcher).summary().catch((e: unknown) => e);
    expect(refused).toBeInstanceOf(ApiError);
    expect(refused).toMatchObject({ status: 401, code: "not-signed-in", hint: "Open the link." });
  });

  it("reads a dry run a newly failing check blocked as its answer, and any other 422 as a refusal", async () => {
    const blocked = { schema: "totalfloat.edit/1", file: "plant.tfdb@main", output: null, applied: [], warnings: [], projects: [] };
    const api = new Api("t", () => Promise.resolve(new Response(JSON.stringify(blocked), { status: 422 })));
    const { answer } = await api.edit({ edits: [], head: "h1", dry_run: true, check: true });
    expect(answer.schema).toBe("totalfloat.edit/1");
    // A blocked edit records nothing, so it mustn't read as recorded.
    await expect(api.edit({ edits: [], head: "h1", check: true })).rejects.toBeInstanceOf(ApiError);
  });

  it("says so when a successful answer isn't JSON", async () => {
    const fetcher: typeof fetch = () =>
      Promise.resolve(new Response("<!doctype html>", { status: 200 }));
    const refused = await new Api(null, fetcher).summary().catch((e: unknown) => e);
    expect(refused).toMatchObject({ code: "bad-answer" });
  });
});
