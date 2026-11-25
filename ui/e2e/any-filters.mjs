// A filter of "any of" groups in a real browser, through playwright-cli,
// against a schedule copied from a synthetic fixture: build two groups, see
// both groups' activities, save it by name, clear it, reload, and apply it by
// name to find the same activities, read back from the file beside the
// schedule.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/any-filters.mjs [--bin target/release/totalfloat] [--screenshot any-filters.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const session = `any-filters-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-any-filters-e2e-"));

function cli(...args) {
  return execFileSync("playwright-cli", [`-s=${session}`, ...args], { encoding: "utf8" });
}

/** Runs `expression` in the page and returns its JSON result. */
function evaluate(expression) {
  const out = cli("eval", expression);
  const match = /### Result\n([\s\S]*?)(?:\n### |$)/.exec(out);
  if (!match) throw new Error(`no result from ${expression}:\n${out}`);
  return JSON.parse(match[1].trim());
}

let failed = false;
function check(name, ok, detail) {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail === undefined ? "" : `: ${JSON.stringify(detail)}`}`);
  if (!ok) failed = true;
}

function pause(ms) {
  execFileSync(process.execPath, ["-e", `setTimeout(() => {}, ${ms})`]);
}

function waitFor(expression, ms = 15_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (evaluate(expression)) return true;
    pause(150);
  }
  return false;
}

/** A real click on the first element matching `selector` that holds `text`. */
function click(selector, text = "") {
  cli("run-code", `async (page) => { await page.locator(${JSON.stringify(selector)}, { hasText: ${JSON.stringify(text)} }).first().click(); }`);
}

/** Types into the field with this label. */
function fill(label, text) {
  cli("run-code", `async (page) => { await page.getByLabel(${JSON.stringify(label)}, { exact: true }).fill(${JSON.stringify(text)}); }`);
}

const COUNT = 'document.querySelector(".grid-count")?.textContent ?? null';
const SAVED = '[...document.querySelectorAll(".filter-builder select[aria-label=\'Saved filter\'] option")].map((o) => o.value)';

let server;
try {
  copyFileSync(join(repo, "fixtures/xer/two-projects.xer"), join(dir, "plant.xer"));

  server = spawn(bin, ["serve", join(dir, "plant.xer")], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
  const link = await new Promise((done, fail) => {
    let out = "";
    server.stdout.on("data", (chunk) => {
      out += chunk;
      const found = /http:\/\/\S+/.exec(out);
      if (found) done(found[0]);
    });
    server.on("exit", () => fail(new Error(`serve stopped: ${out}`)));
  });

  cli("open", link);
  cli("resize", "1500", "900");
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("every activity shows", waitFor(`(${COUNT}) === "3 of 3"`), evaluate(COUNT));

  // Any of: code A (one activity), or M1 in the ID or name (another).
  cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitem", { name: "More conditions…" }).click(); }');
  check("the builder opens", waitFor('!!document.querySelector(".filter-builder")'));
  click(".filter-builder button", "Add an “any of” group");
  cli("run-code", 'async (page) => { await page.getByLabel("Add a condition, group 1", { exact: true }).selectOption("code"); }');
  cli("run-code", 'async (page) => { await page.getByLabel("Activity code, group 1 type", { exact: true }).selectOption("Synthetic global codes"); }');
  fill("Activity code, group 1 value", "A");
  click(".filter-builder button", "Add an “any of” group");
  cli("run-code", 'async (page) => { await page.getByLabel("Add a condition, group 2", { exact: true }).selectOption("text"); }');
  fill("ID or name contains, group 2 value", "M1");
  click(".filter-builder button", "Apply");
  check("either group's activity shows", waitFor(`(${COUNT}) === "2 of 3"`), evaluate(COUNT));
  fill("Filter name", "A or M1");
  click(".filter-builder button", "Save as");
  check("it's listed", waitFor(`JSON.stringify(${SAVED}) === '["A or M1"]'`), evaluate(SAVED));
  const file = join(dir, "plant.filters", "A or M1.json");
  check("as a file beside the schedule", existsSync(file));
  const saved = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
  check(
    "holding the groups as any",
    saved?.schema === "totalfloat.filter/1" &&
      JSON.stringify(saved?.any) === JSON.stringify([{ code: "Synthetic global codes=A" }, { text: "M1" }]),
    saved,
  );

  // Clear it, reload, and apply it by name: the same two activities again.
  click(".filter-builder button", "Clear");
  check("cleared, every activity shows", waitFor(`(${COUNT}) === "3 of 3"`), evaluate(COUNT));
  cli("reload");
  check("the grid loads again", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("every activity shows after the reload", waitFor(`(${COUNT}) === "3 of 3"`), evaluate(COUNT));
  cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitem", { name: "More conditions…" }).click(); }');
  check("the saved filter is still listed", waitFor(`JSON.stringify(${SAVED}) === '["A or M1"]'`), evaluate(SAVED));
  click(".filter-builder button", "Apply saved");
  check("applying it finds the same two activities", waitFor(`(${COUNT}) === "2 of 3"`), evaluate(COUNT));
  check(
    "both groups show in the builder",
    waitFor('document.querySelectorAll(".filter-builder fieldset.filter-group").length === 2'),
  );

  if (shot) cli("screenshot", `--filename=${shot}`);
} catch (error) {
  failed = true;
  console.error(error);
} finally {
  try {
    cli("close");
  } catch {
    // Already closed.
  }
  server?.kill();
  if (!failed) rmSync(dir, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
