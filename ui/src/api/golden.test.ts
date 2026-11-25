// Each golden example in ./golden/ is a real answer from the server
// (crates/cli/src/serve/tests.rs writes them). Here each one must fit its
// published schema, which the app's types are generated from, and come back
// from the client method for its endpoint as sent.
import { Ajv2020 } from "ajv/dist/2020";
import { describe, expect, it } from "vitest";

import { Api, ApiError } from "./client";
import { parseEvent } from "../events";

const schemaFiles = import.meta.glob<object>("../../../docs/schemas/*.schema.json", {
  eager: true,
  import: "default",
});
/** A made-up base for the schemas' `$id`s, which aren't URIs of their own. */
const BASE = "https://schemas.totalfloat.invalid/";
const goldens = import.meta.glob<unknown>("./golden/*.json", { eager: true, import: "default" });

/**
 * The published schemas. Their `$id`s aren't absolute URIs, so references
 * between files would resolve against the referring schema's `$id`. Each
 * `$id` is put under one made-up base URI, and each reference to a file
 * becomes the full URI of that file's `$id`.
 */
function validator(): Ajv2020 {
  const byFile = new Map<string, { $id: string }>();
  for (const [path, schema] of Object.entries(schemaFiles)) {
    byFile.set(path.split("/").pop() ?? path, schema as { $id: string });
  }
  const ajv = new Ajv2020({ strict: false, validateFormats: false, allErrors: true });
  for (const schema of byFile.values()) {
    const text = JSON.stringify({ ...schema, $id: `${BASE}${schema.$id}` }).replace(
      /"\$ref":"([a-z0-9-]+\.schema\.json)(#[^"]*)?"/g,
      (_match, file: string, pointer: string | undefined) =>
        `"$ref":"${BASE}${byFile.get(file)?.$id ?? file}${pointer ?? ""}"`,
    );
    ajv.addSchema(JSON.parse(text) as object);
  }
  return ajv;
}

function golden(name: string): unknown {
  const found = goldens[`./golden/${name}.json`];
  if (found === undefined) throw new Error(`no golden example ${name}`);
  return found;
}

/** A client whose server answers `body` with `status`. */
function answering(body: unknown, status = 200): Api {
  return new Api("token", () =>
    Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "x-totalfloat-head": "h" } })),
  );
}

const names = Object.keys(goldens).map((path) => path.replace("./golden/", "").replace(".json", ""));

describe("golden examples", () => {
  const ajv = validator();

  it("cover the schemas the app reads", () => {
    expect(names.length).toBeGreaterThanOrEqual(22);
  });

  it.each(names)("%s fits its published schema", (name) => {
    const schema = ajv.getSchema(`${BASE}totalfloat.${name.replace(/-1$/, "")}/1`);
    expect(schema, `no schema totalfloat.${name}`).toBeDefined();
    const examples = name === "event-1" ? (golden(name) as unknown[]) : [golden(name)];
    expect(examples.length).toBeGreaterThan(0);
    for (const example of examples) {
      const valid = schema?.(example);
      expect(schema?.errors ?? [], `${name} doesn't fit its schema`).toEqual([]);
      expect(valid).toBe(true);
    }
  });
});

describe("the client parses each golden example", () => {
  const reads: [string, (api: Api) => Promise<unknown>][] = [
    ["summary-1", (api) => api.summary()],
    ["rows-1", (api) => api.rows()],
    ["links-1", (api) => api.links()],
    ["wbs-1", (api) => api.wbs()],
    ["explain-1", (api) => api.explain("A2", "early_start")],
    ["check-1", (api) => api.check()],
    ["query-1", (api) => api.query({ status: "not-started", columns: "id,code,name" })],
    ["fields-1", (api) => api.fields()],
    ["edit-1", async (api) => (await api.edit({ edits: [], head: "h" })).answer],
    ["layouts-1", (api) => api.layouts()],
    ["layout-1", (api) => api.layout("Look ahead")],
    ["filters-1", (api) => api.filters()],
    ["filter-1", (api) => api.filter("Critical design")],
    ["history-log-1", (api) => api.log()],
    ["changeset-1", (api) => api.show("main")],
    ["history-diff-1", (api) => api.diff()],
    ["history-proposals-1", (api) => api.proposals()],
    ["history-branches-1", async (api) => (await api.branches()).answer],
    ["history-baselines-1", async (api) => (await api.baselines()).answer],
    ["history-branch-1", async (api) => (await api.branch({ action: "create", name: "b" })).answer],
    ["history-baseline-1", async (api) => (await api.baseline({ action: "create", name: "b" })).answer],
    ["history-merge-1", async (api) => (await api.merge({ branch: "b", dry_run: true })).answer],
    ["history-review-1", async (api) => (await api.accept({ proposal: "p" })).answer],
    ["history-revert-1", async (api) => (await api.revert()).answer],
    ["risk-analysis-1", (api) => api.risk({ inputs: {} })],
    ["portfolio-show-1", (api) => api.portfolio()],
    ["portfolio-capacity-1", (api) => api.capacity({ by: "week" })],
  ];

  it.each(reads)("%s", async (name, read) => {
    expect(await read(answering(golden(name)))).toEqual(golden(name));
  });

  it("covers every golden example", () => {
    const covered = new Set([...reads.map(([name]) => name), "error-1", "event-1"]);
    expect(names.filter((name) => !covered.has(name))).toEqual([]);
  });

  it("error-1 becomes an ApiError with every problem", async () => {
    const refused = await answering(golden("error-1"), 422)
      .rows()
      .catch((e: unknown) => e);
    expect(refused).toBeInstanceOf(ApiError);
    const error = refused as ApiError;
    expect(error.status).toBe(422);
    expect(error.code).toBe("edits-refused");
    expect(error.problems.length).toBe(2);
    expect(error.problems.map((p) => p.code)).toEqual(["no-such-activity", "no-such-activity"]);
  });

  it("event-1's events are each read as an event", () => {
    for (const event of golden("event-1") as unknown[]) {
      const read = parseEvent(JSON.stringify(event));
      expect(read).toEqual(event);
    }
  });
});
