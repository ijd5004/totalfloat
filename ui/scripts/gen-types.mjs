// Generates TypeScript types from the published JSON schemas in
// docs/schemas/, so the app's types can't drift from what the server sends.
// Run by `pnpm gen`, before every type check, test and build. The output in
// src/api/schemas/ isn't committed.
import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { compileFromFile } from "json-schema-to-typescript";

const here = dirname(fileURLToPath(import.meta.url));
const schemas = join(here, "..", "..", "docs", "schemas");
const out = join(here, "..", "src", "api", "schemas");

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });
const files = (await readdir(schemas)).filter((f) => f.endsWith(".schema.json")).sort();
for (const file of files) {
  const ts = await compileFromFile(join(schemas, file), {
    cwd: schemas,
    bannerComment: `// Generated from docs/schemas/${file} by scripts/gen-types.mjs. Don't edit.`,
    additionalProperties: false,
    strictIndexSignatures: true,
    unreachableDefinitions: false,
    format: false,
  });
  await writeFile(join(out, file.replace(".schema.json", ".ts")), ts);
}
// The golden examples, each typed as the client reads it (`Answers` in
// src/api/types.ts). Written as object literals, so the type check holds
// every field, extra ones included, to the generated types.
const golden = join(here, "..", "src", "api", "golden");
const examples = (await readdir(golden)).filter((f) => f.endsWith(".json")).sort();
let typed = `// Generated from src/api/golden/ by scripts/gen-types.mjs. Don't edit.
import type { Answers } from "../types";

export const golden: Answers = {
`;
for (const file of examples) {
  const text = await readFile(join(golden, file), "utf8");
  typed += `  ${JSON.stringify(file.replace(".json", ""))}: ${text.trim()},\n`;
}
typed += "};\n";
await writeFile(join(out, "golden.ts"), typed);

console.log(`Generated types for ${files.length} schemas and ${examples.length} golden examples in src/api/schemas/.`);
