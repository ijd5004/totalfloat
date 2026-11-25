// The grid's filter builder and grouping, in a real browser, through
// playwright-cli, against the synthetic fixture with activity codes and a
// user field: build a filter by code, see only what `query` finds, then
// group the rows by a code type and by the user field. Then, on the plant
// fixture with a second node coded 2 in another branch, a WBS condition
// takes the path the grid shows and picks that node alone.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/filters.mjs [--bin target/release/totalfloat] [--screenshot filters.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const session = `filters-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-filters-e2e-"));

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

/** Picks an option in the select with this label. */
function choose(label, value) {
  cli(
    "run-code",
    `async (page) => { const select = page.getByLabel(${JSON.stringify(label)}, { exact: true }); if (!(await select.count()) && ${JSON.stringify(label)}.includes("roup")) await page.getByRole("button", { name: /^Grouping:/ }).click(); await select.selectOption(${JSON.stringify(value)}); }`,
  );
}

/** What the grid lists, top to bottom: each activity's ID, and each band as `[its title]`. */
const LISTED = `[...document.querySelectorAll("#grid-mirror tbody tr")].map((tr) => {
  const cell = tr.querySelector("td");
  return tr.getAttribute("aria-expanded") === null ? cell.textContent : "[" + cell.textContent.split(",")[0] + "]";
})`;
const COUNT = 'document.querySelector(".grid-count").textContent';

function listedIs(expected) {
  return waitFor(`JSON.stringify(${LISTED}) === ${JSON.stringify(JSON.stringify(expected))}`);
}

let server;
/** Serves a copy of a fixture and gives the sign-in link. */
function serve(fixture, name) {
  const from = isAbsolute(fixture) ? fixture : join(repo, fixture);
  if (from !== join(dir, name)) copyFileSync(from, join(dir, name));
  server = spawn(bin, ["serve", join(dir, name)], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
  return new Promise((done, fail) => {
    let out = "";
    server.stdout.on("data", (chunk) => {
      out += chunk;
      const found = /http:\/\/\S+/.exec(out);
      if (found) done(found[0]);
    });
    server.on("exit", () => fail(new Error(`serve stopped: ${out}`)));
  });
}

try {
  const link = await serve("fixtures/xer/two-projects.xer", "codes.xer");

  cli("open", link);
  cli("resize", "1500", "900");
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("every activity shows", waitFor(`(${COUNT}) === "3 of 3"`), evaluate(COUNT));

  // Build a filter: the global code's value A.
  cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitem", { name: "More conditions…" }).click(); }');
  check("the builder opens", waitFor('!!document.querySelector(".filter-builder")'));
  choose("Add a condition", "code");
  choose("Activity code type", "Synthetic global codes");
  fill("Activity code value", "A");
  click(".filter-builder button", "Apply");
  check("only what query finds shows", waitFor(`(${COUNT}) === "1 of 3"`), evaluate(COUNT));
  check("the filter shows as a chip", waitFor('document.querySelectorAll("[aria-label=\'Filters applied\'] .chip").length === 1'));
  const filtered = evaluate(LISTED);
  check("which is A1", filtered.includes("A1") && !filtered.includes("B1") && !filtered.includes("M1"), filtered);

  // A refused filter is said, and the applied one stays.
  choose("Add a condition", "wbs");
  fill("WBS value", "NO-SUCH-WBS");
  click(".filter-builder button", "Apply");
  check("a refusal is said", waitFor('/no WBS|WBS/.test(document.querySelector(".filter-problem")?.textContent ?? "")'), evaluate('document.querySelector(".filter-problem")?.textContent ?? null'));
  check("and the filter applied stays", evaluate(COUNT) === "1 of 3", evaluate(COUNT));
  click(".filter-builder button", "Clear");
  check("clearing shows every activity", waitFor(`(${COUNT}) === "3 of 3"`), evaluate(COUNT));
  click(".filter-builder button", "Close");

  // Group by the global code type, then by the user field.
  choose("Group by", "code:Synthetic global codes");
  check(
    "bands by the code's values, with none last",
    listedIs(["[Synthetic global codes: A]", "A1", "[Synthetic global codes: (no value)]", "M1", "B1"]),
    evaluate(LISTED),
  );
  choose("Group by", "udf:Synthetic task number");
  check(
    "bands by the user field's values",
    listedIs(["[Synthetic task number: 1.25]", "A1", "[Synthetic task number: (no value)]", "M1", "B1"]),
    evaluate(LISTED),
  );
  if (shot) cli("screenshot", `--filename=${shot}`);
  choose("Group by", "none");
  check("and no bands", listedIs(["A1", "M1", "B1"]), evaluate(LISTED));

  // A WBS path, as the grid shows it, picks that node alone, where another
  // branch has a node with the same code; the code alone picks both.
  server.removeAllListeners("exit");
  server.kill();
  // The plant schedule with a second node coded 2, under node 1 (`wbs_id` 7), holding A9.
  execFileSync(bin, ["import", join(repo, "fixtures/msproject/plant.xml"), "-o", join(dir, "plant.xer")], { cwd: dir });
  execFileSync(bin, ["edit", join(dir, "plant.xer"), "--add-wbs", "2;Yard drains;1", "--add", "A9;Drain;7;3d", "-o", join(dir, "shared.xer")], { cwd: dir });
  cli("goto", await serve(join(dir, "shared.xer"), "shared.xer"));
  check("the second schedule loads", waitFor(`(${COUNT}) === "6 of 6"`), evaluate(COUNT));
  const wbsFilter = (value) => {
    if (!evaluate('!!document.querySelector(".filter-builder")')) {
      cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitem", { name: "More conditions…" }).click(); }');
      waitFor('!!document.querySelector(".filter-builder")');
    }
    click(".filter-builder button", "Clear");
    choose("Add a condition", "wbs");
    fill("WBS value", value);
    click(".filter-builder button", "Apply");
  };
  const shownCodes = `[...document.querySelectorAll("#grid-mirror tbody tr")].filter((tr) => tr.getAttribute("aria-expanded") === null).map((tr) => tr.cells[0].textContent).sort().join(" ")`;
  wbsFilter("Plant & Yard Upgrade.2");
  check("the path the grid shows picks that node", waitFor(`(${COUNT}) === "2 of 6"`) && evaluate(shownCodes) === "A5 A6", [evaluate(COUNT), evaluate(shownCodes)]);
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-wbs-path.png`);
  wbsFilter("2");
  check("its code alone picks both nodes with it", waitFor(`(${COUNT}) === "3 of 6"`) && evaluate(shownCodes) === "A5 A6 A9", [evaluate(COUNT), evaluate(shownCodes)]);
  wbsFilter("Plant & Yard Upgrade.3");
  const problem = '(document.querySelector(".filter-problem")?.textContent ?? "")';
  check(
    "a path that isn't there is refused in two sentences, naming no P6 table",
    waitFor(`${problem}.includes("Plant & Yard Upgrade.3. Give")`) && !evaluate(problem).includes("PROJWBS"),
    evaluate(problem),
  );
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-wbs-refused.png`);
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
