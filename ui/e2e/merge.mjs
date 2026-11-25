// Merging a branch from the app, in a real browser, through playwright-cli,
// against a store made from a synthetic fixture:
//   - a clean merge, onto a main that moved on after the branch was made,
//     previews only the branch's edit and records a merge changeset on main;
//   - a merge where both branches changed the same duration lists the clash,
//     with both values, before anything is merged, and leaves both branches
//     as they were.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/merge.mjs [--bin target/release/totalfloat] [--screenshot merge.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const session = `merge-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-merge-e2e-"));
const store = join(dir, "plant.tfdb");

function totalfloat(...args) {
  return execFileSync(bin, args, { encoding: "utf8", cwd: dir });
}

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

/** A real click on the button with this accessible name. */
function press(name) {
  cli("run-code", `async (page) => { await page.getByRole("button", { name: ${JSON.stringify(name)}, exact: true }).click(); }`);
}

/** A branch's changesets, newest first, as `history log` lists them. */
function log(branch = "main") {
  return totalfloat("history", "log", store, "--branch", branch)
    .split("\n")
    .filter((line) => /^[0-9a-f]{10} /.test(line));
}

/** Makes a change on a branch from the command line, leaving main checked out. */
function editOn(branch, ...args) {
  totalfloat("history", "switch", store, branch);
  totalfloat("edit", store, ...args);
  totalfloat("history", "switch", store, "main");
}

const NOTICE = 'document.querySelector(".edit-bar .notice").textContent';

/** An expression for the mirror's text for an activity's cell, by its ID and the column's title. */
function cellIs(code, title) {
  return `(() => {
    const titles = [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const row = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    const td = row && row.cells[titles.indexOf(${JSON.stringify(title)})];
    return td ? td.textContent : null;
  })()`;
}

const CLASHES = '(document.querySelector(".merge-dialog .review-conflicts")?.textContent ?? "")';

let server;
try {
  // A store: the fixture's two activities, and two more chained after them.
  copyFileSync(join(repo, "fixtures/xer/batches/001/c-start-after.xer"), join(dir, "plant.xer"));
  totalfloat("history", "init", "plant.xer", "-m", "Start");
  const wbs = totalfloat("query", store, "--columns", "code,wbs").split("\n")[1].split(/\s+/)[1];
  totalfloat(
    "edit", store,
    "--add", `A3;Synthetic activity 3;${wbs};3d`,
    "--add", `A4;Synthetic activity 4;${wbs};2d`,
    "--link", "A2>A3", "--link", "A3>A4", "-m", "Chain",
  );
  // A branch with a change of its own, and work on main since that doesn't
  // touch it: a rename, and an activity added and linked. A clean merge,
  // recorded as a merge changeset, that keeps main's later work.
  totalfloat("history", "branch", store, "what-if");
  editOn("what-if", "--duration", "A3=5d", "-m", "Pour takes five days");
  totalfloat("edit", store, "--name", "A1=Mobilise", "-m", "Rename");
  totalfloat("edit", store, "--add", `A5;Synthetic activity 5;${wbs};1d`, "--link", "A4>A5", "-m", "Handover");

  server = spawn(bin, ["serve", store], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
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

  // A clean merge: the compare first, then a merge changeset on main.
  click(".branch-button");
  check("the picker offers the merge", waitFor('!!document.querySelector("[aria-label=\'Merge what-if into main\']")'));
  press("Merge what-if into main");
  const PREVIEW = '(document.querySelector(".merge-dialog .history-result")?.textContent ?? "")';
  check(
    "the dialog previews the branch's edit",
    waitFor(`${PREVIEW}.includes("A3") && ${PREVIEW}.includes("3 days → 5 days")`),
    evaluate('document.querySelector(".merge-dialog")?.textContent ?? null'),
  );
  check(
    "and nothing of main's later work, which the merge keeps",
    !/A1|A5|Deleted|deleted/.test(evaluate(PREVIEW)),
    evaluate(PREVIEW),
  );
  const before = log().length;
  click(".merge-dialog button.primary");
  check("the merge is recorded", waitFor(`${NOTICE}.startsWith("Merged what-if into main as ")`), evaluate(NOTICE));
  check("as a merge changeset on main", log().length === before + 1 && /\bmerge\b/.test(log()[0]), log()[0]);
  check(
    "and the grid shows both sides",
    waitFor(`${cellIs("A3", "Original (d)")} === "5" && ${cellIs("A1", "Activity name")} === "Mobilise" && ${cellIs("A5", "Original (d)")} === "1"`),
    [evaluate(cellIs("A3", "Original (d)")), evaluate(cellIs("A1", "Activity name")), evaluate(cellIs("A5", "Original (d)"))],
  );

  // Both branches change A2's duration: the merge is refused, and lists the clash.
  totalfloat("history", "branch", store, "crane");
  editOn("crane", "--duration", "A2=7d", "-m", "Crane early");
  totalfloat("edit", store, "--duration", "A2=8d", "-m", "Crane late");
  check("the page shows main's change", waitFor(`${cellIs("A2", "Original (d)")} === "8"`), evaluate(cellIs("A2", "Original (d)")));
  const [mainBefore, craneBefore] = [log()[0], log("crane")[0]];
  click(".branch-button");
  check("the picker offers that merge too", waitFor('!!document.querySelector("[aria-label=\'Merge crane into main\']")'));
  press("Merge crane into main");
  check(
    "the clash is listed with both values before merging",
    waitFor(`${CLASHES}.includes("A2") && ${CLASHES}.includes("main: 8 days") && ${CLASHES}.includes("crane: 7 days")`),
    evaluate(CLASHES),
  );
  check("with the changes that set them", evaluate(CLASHES).includes("Crane late") && evaluate(CLASHES).includes("Crane early"), evaluate(CLASHES));
  check("and there's nothing to merge", evaluate('document.querySelector(".merge-dialog button.primary").disabled'));
  check("and both branches are as they were", log()[0] === mainBefore && log("crane")[0] === craneBefore, [log()[0], log("crane")[0]]);

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
