// Grouping by several levels in a real browser, through playwright-cli:
// group a schedule by the WBS, then by an activity code, see the code's
// bands within the WBS bands, save the layout from the command palette,
// reload, and see the same bands, read back from the file beside the
// schedule. Give a corpus schedule with --file; it defaults to a
// synthetic fixture. Nothing from the file is printed but counts.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/group-levels.mjs [--file plan.xer] [--bin target/release/totalfloat]
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
const fileAt = flags.indexOf("--file");
const source = resolve(fileAt >= 0 ? flags[fileAt + 1] : join(repo, "fixtures/xer/two-projects.xer"));
const session = `group-levels-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-group-levels-e2e-"));

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
// Read with the grouping panel opened if it's closed.
const THEN = '(() => { const open = () => document.querySelector(".grid-tools select[aria-label=\'Then by, level 2\']"); if (!open()) document.querySelector("[aria-label^=\'Grouping:\']")?.click(); return open()?.value ?? null; })()';
// The bands the screen-reader mirror lists, top to bottom: what each says.
const BANDS = '[...document.querySelectorAll("#grid-mirror tbody tr[aria-expanded]")].map((tr) => tr.textContent)';
// The code types the Then menu offers.
const CODES =
  '(() => { if (!document.querySelector(".grid-tools select[aria-label=\'Then group by\']")) document.querySelector("[aria-label^=\'Grouping:\']")?.click(); return [...document.querySelectorAll(".grid-tools select[aria-label=\'Then group by\'] optgroup[label=\'Activity codes\'] option")].map((o) => [o.value, o.textContent]); })()';

let server;
try {
  copyFileSync(source, join(dir, "plan.xer"));

  server = spawn(bin, ["serve", join(dir, "plan.xer")], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
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
  const wbsBands = evaluate(BANDS);
  check("grouped by the WBS", wbsBands.length > 0 && wbsBands.every((b) => b.startsWith("WBS ")), { bands: wbsBands.length });

  // Then by the activity code type the menu offers first.
  check("an activity code type to group by", waitFor(`(${CODES}).length > 0`), { types: evaluate(CODES).length });
  const [[code, title]] = evaluate(CODES);
  cli("run-code", `async (page) => { await page.getByLabel("Then group by", { exact: true }).selectOption(${JSON.stringify(code)}); }`);
  check("then by the code", waitFor(`(${THEN}) === ${JSON.stringify(code)}`));
  check("its bands within the WBS bands", waitFor(`(${BANDS}).some((b) => b.startsWith(${JSON.stringify(`${title}: `)}))`));
  const bands = evaluate(BANDS);
  const inner = bands.filter((b) => b.startsWith(`${title}: `)).length;
  check("both kinds of band show", inner > 0 && bands.length > inner, { bands: bands.length, codeBands: inner });
  const firstWbs = bands.findIndex((b) => b.startsWith("WBS "));
  const firstCode = bands.findIndex((b) => b.startsWith(`${title}: `));
  check("a WBS band comes first", firstWbs === 0 && firstCode > 0, { firstWbs, firstCode });

  // Save it from the palette.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+k");
  cli("type", "Save layout as");
  cli("press", "Enter");
  check("the save dialog opens", waitFor('!!document.querySelector(".layout-dialog")'));
  fill("Name", "WBS then code");
  click(".layout-dialog button[type=submit]", "Save");
  check("it's saved", waitFor(`(${NOTICE}) === "Saved layout WBS then code."`), evaluate(NOTICE));
  const file = join(dir, "plan.layouts", "WBS then code.json");
  const saved = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
  check(
    "the file holds both levels",
    saved?.group === "wbs" && JSON.stringify(saved?.then_by) === JSON.stringify([code]),
    saved && { group: saved.group, levels: saved.then_by?.length },
  );

  // Reload: the same bands, read back from the file.
  cli("reload");
  check("the grid loads again", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("then by the code again", waitFor(`(${THEN}) === ${JSON.stringify(code)}`));
  check(
    "the same bands after a reload",
    waitFor(`JSON.stringify(${BANDS}) === ${JSON.stringify(JSON.stringify(bands))}`),
    { before: bands.length, after: evaluate(BANDS).length },
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
