// Layouts as files, in a real browser, through playwright-cli, against a
// schedule copied from a synthetic fixture: change the columns, group by an
// activity code, build a filter, save the layout from the command palette,
// reload, and see the same view, read back from the file beside the schedule.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/layouts.mjs [--bin target/release/totalfloat] [--screenshot layouts.png]
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
const session = `layouts-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-layouts-e2e-"));

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

const NOTICE = 'document.querySelector(".edit-bar .notice").textContent';
const TITLES = '[...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent)';
// The grouping, as the grouping panel's first select holds it, read with the panel opened if it's closed.
const GROUP = '(() => { const open = () => document.querySelector(".grid-tools select[aria-label=\'Group by\']"); if (!open()) document.querySelector("[aria-label^=\'Grouping:\']")?.click(); return open()?.value ?? null; })()';
const COUNT = 'document.querySelector(".grid-count")?.textContent ?? null';
const FILTERED = 'document.querySelectorAll("[aria-label=\'Filters applied\'] .chip").length === 1';
const CODE = "code:Synthetic global codes";
const WIDTH = '[...document.querySelectorAll(".grid-head")].find((h) => h.textContent.startsWith("Activity ID"))?.getBoundingClientRect().width';

/** Focuses a column's header button, by the start of its title. */
function header(title) {
  evaluate(`([...document.querySelectorAll(".grid-head-button")].find((b) => b.getAttribute("aria-label").startsWith(${JSON.stringify(title)})).focus(), true)`);
}

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
  const before = evaluate(TITLES);

  // Move Original one to the right, widen Activity ID, group by an activity
  // code, and build a filter: the code's value A.
  header("Original");
  cli("press", "Alt+Shift+ArrowRight");
  header("Activity ID");
  cli("press", "Alt+ArrowRight");
  cli("press", "Alt+ArrowRight");
  cli("run-code", `async (page) => { const select = page.getByLabel("Group by", { exact: true }); if (!(await select.count())) await page.getByRole("button", { name: /^Grouping:/ }).click(); await select.selectOption(${JSON.stringify(CODE)}); }`);
  cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitem", { name: "More conditions…" }).click(); }');
  check("the builder opens", waitFor('!!document.querySelector(".filter-builder")'));
  cli("run-code", 'async (page) => { await page.getByLabel("Add a condition", { exact: true }).selectOption("code"); }');
  cli("run-code", 'async (page) => { await page.getByLabel("Activity code type", { exact: true }).selectOption("Synthetic global codes"); }');
  fill("Activity code value", "A");
  click(".filter-builder button", "Apply");
  check("the filter applies", waitFor(`(${COUNT}) === "1 of 3"`), evaluate(COUNT));
  click(".filter-builder button", "Close");
  const changed = evaluate(TITLES);
  const width = evaluate(WIDTH);
  check("the columns moved", changed.indexOf("Original (d)") === before.indexOf("Original (d)") + 1, changed);
  check("grouped by the code", waitFor(`(${GROUP}) === ${JSON.stringify(CODE)}`), evaluate(GROUP));

  // Save it from the palette.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+k");
  cli("type", "Save layout as");
  cli("press", "Enter");
  check("the save dialog opens", waitFor('!!document.querySelector(".layout-dialog")'));
  fill("Name", "Look ahead");
  click(".layout-dialog button[type=submit]", "Save");
  check("it's saved", waitFor(`(${NOTICE}) === "Saved layout Look ahead."`), evaluate(NOTICE));
  const file = join(dir, "plant.layouts", "Look ahead.json");
  check("as a file beside the schedule", existsSync(file));
  const saved = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
  check("holding the grouping and columns", saved?.group === CODE && saved?.columns?.[0]?.field === "code", saved);
  check("and the built filter, as query's parameters", saved?.filters?.built?.code === "Synthetic global codes=A", saved?.filters);
  check("and the Gantt fitted, as fit", saved?.gantt?.zoom === "fit", saved?.gantt);

  // Reload: the same view, read back from the file.
  cli("reload");
  check("the grid loads again", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("the columns are as they were saved", waitFor(`JSON.stringify(${TITLES}) === ${JSON.stringify(JSON.stringify(changed))}`), evaluate(TITLES));
  check("grouped by the code again", waitFor(`(${GROUP}) === ${JSON.stringify(CODE)}`), evaluate(GROUP));
  check("with the filter applied again", waitFor(`(${COUNT}) === "1 of 3" && ${FILTERED}`), [evaluate(COUNT), evaluate(FILTERED)]);
  check("Activity ID is as wide", Math.abs(evaluate(WIDTH) - width) < 1, [evaluate(WIDTH), width]);
  check("the Gantt reopens fitted", waitFor('document.querySelector(".gantt-body")?.dataset.zoom === "fit"'));

  // Group by status and sort on three keys: Activity ID, then
  // Shift-click Name and Start, saved and read back.
  cli("run-code", 'async (page) => { const select = page.getByLabel("Group by", { exact: true }); if (!(await select.count())) await page.getByRole("button", { name: /^Grouping:/ }).click(); await select.selectOption("status"); }');
  check("grouped by status", waitFor(`(${GROUP}) === "status"`), evaluate(GROUP));
  const shiftClick = (title) =>
    cli(
      "run-code",
      `async (page) => { await page.locator(".grid-head-button", { hasText: ${JSON.stringify(title)} }).first().click({ modifiers: ["Shift"] }); }`,
    );
  click(".grid-head-button", "Activity ID");
  shiftClick("Activity name");
  shiftClick("Start");
  const KEYS = '[...document.querySelectorAll(".grid-head-button")].map((b) => b.getAttribute("aria-label")).filter((l) => l.includes(", key "))';
  check("three sort keys", waitFor(`(${KEYS}).length === 3`), evaluate(KEYS));
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  // Zoomed in from fit, the layout keeps a fixed scale.
  cli("press", "Alt+Equal");
  check("zooming in leaves fit", waitFor('document.querySelector(".gantt-body")?.dataset.zoom !== "fit"'));
  cli("press", "Control+k");
  cli("type", "Save layout as");
  cli("press", "Enter");
  check("the save dialog opens again", waitFor('!!document.querySelector(".layout-dialog")'));
  fill("Name", "By status");
  click(".layout-dialog button[type=submit]", "Save");
  check("saved again", waitFor(`(${NOTICE}) === "Saved layout By status."`), evaluate(NOTICE));
  const second = join(dir, "plant.layouts", "By status.json");
  const kept = existsSync(second) ? JSON.parse(readFileSync(second, "utf8")) : null;
  check(
    "the file holds the status grouping and the three keys in order",
    kept?.group === "status" && JSON.stringify(kept?.sort?.map((k) => k.field)) === JSON.stringify(["code", "name", "start"]),
    kept && { group: kept.group, sort: kept.sort },
  );
  check("and a fixed scale", typeof kept?.gantt?.zoom?.px_per_day === "number", kept?.gantt);
  const keys = evaluate(KEYS);
  cli("reload");
  check("the grid loads a third time", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("grouped by status after a reload", waitFor(`(${GROUP}) === "status"`), evaluate(GROUP));
  check("the same three keys after a reload", waitFor(`JSON.stringify(${KEYS}) === ${JSON.stringify(JSON.stringify(keys))}`), evaluate(KEYS));

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
