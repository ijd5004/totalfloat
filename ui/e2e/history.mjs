// History in the app, in a real browser, through playwright-cli, against a
// store made from a synthetic fixture:
//   - a branch is made from the branch picker, edited, and compared with main;
//   - a baseline is made from an entry, then the head is edited and compared
//     with it;
//   - changes recorded with no reason are titled in words ("A2 duration");
//   - an entry is taken out (reverted);
//   - a point is exported, and its bytes match `history export`;
//   - on a log longer than the window, a change's and a baseline's Compare
//     with now bring the comparison into view.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/history.mjs [--bin target/release/totalfloat] [--screenshot history.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const session = `history-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-history-e2e-"));
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

/** Types into the field with this label. */
function fill(label, text) {
  cli("run-code", `async (page) => { await page.getByLabel(${JSON.stringify(label)}, { exact: true }).fill(${JSON.stringify(text)}); }`);
}

/** Chooses an option of the select with this label, by its value. */
function choose(label, value) {
  cli("run-code", `async (page) => { await page.getByLabel(${JSON.stringify(label)}, { exact: true }).selectOption(${JSON.stringify(value)}); }`);
}

/** The changesets on the checked-out branch, newest first, as `history log` lists them. */
function log() {
  return totalfloat("history", "log", store)
    .split("\n")
    .filter((line) => /^[0-9a-f]{10} /.test(line));
}

const NOTICE = 'document.querySelector(".edit-bar .notice").textContent';
const RESULT = '(document.querySelector(".history-result")?.textContent ?? "")';

/** An expression for the mirror's text for an activity's cell, by its ID and the column's title. */
function cellIs(code, title) {
  return `(() => {
    const titles = [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const row = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    const td = row && row.cells[titles.indexOf(${JSON.stringify(title)})];
    return td ? td.textContent : null;
  })()`;
}

/** Puts the active cell on an activity's cell, from the keyboard. */
function goTo(code, title) {
  const [line, column] = evaluate(`(() => {
    const titles = [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const row = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    return [Number(row.getAttribute("aria-rowindex")) - 2, titles.indexOf(${JSON.stringify(title)})];
  })()`);
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  for (let i = 0; i < line; i++) cli("press", "ArrowDown");
  for (let i = 0; i < column; i++) cli("press", "ArrowRight");
}

/**
 * Types a value into an activity's cell and records it: Enter previews, Enter
 * again records. The grid is in the Schedule view, so it goes there and back
 * to History.
 */
function edit(code, title, text) {
  cli("press", "Alt+Shift+G");
  waitFor('!document.querySelector(".view-page:not([hidden])")');
  goTo(code, title);
  for (const key of text) cli("press", key);
  cli("press", "Enter");
  check(`${code}'s edit is previewed`, waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  cli("press", "Enter");
  check(`${code}'s edit is recorded`, waitFor(`${NOTICE}.startsWith("Recorded ")`), evaluate(NOTICE));
  cli("press", "Alt+h");
  waitFor('!!document.querySelector(".history-log .history-entry")');
}

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

  // The store itself, not a branch of it, so the page follows the branch switched to.
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

  // The History view, from its key.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+h");
  check("Alt+H opens the History view", waitFor('!!document.querySelector(".history-log .history-entry")'));
  check(
    "it lists the changes newest first",
    waitFor('[...document.querySelectorAll(".history-entry .history-message")].map((m) => m.textContent).join("|") === "Chain|Startimport"'),
    evaluate('[...document.querySelectorAll(".history-entry .history-message")].map((m) => m.textContent)'),
  );

  // A branch, from the picker, switched to.
  click(".branch-button");
  check("the picker lists main", waitFor('[...document.querySelectorAll(".branch-name")].some((b) => b.textContent.includes("main"))'));
  fill("New branch from what's shown", "what-if");
  click("button", "Create and switch");
  check(
    "the title bar shows the new branch",
    waitFor('document.querySelector(".branch-button .fact-value")?.textContent.startsWith("what-if")'),
    evaluate('document.querySelector(".branch-button .fact-value")?.textContent ?? null'),
  );
  check("the store switched to it", /^\* what-if/m.test(totalfloat("history", "branches", store)), totalfloat("history", "branches", store));

  // An edit on it, compared with main.
  const mainChanges = log().length;
  edit("A2", "Original (d)", "6");
  check("the edit is on what-if", log().length === mainChanges + 1, log()[0]);
  // Recorded with no reason, so titled by what it did, in words.
  const TITLES = '[...document.querySelectorAll(".history-entry .history-message")].map((m) => m.textContent)';
  check("a change with no reason is titled in words", waitFor(`${TITLES}.includes("A2 duration")`), evaluate(TITLES));
  const NAMED = '[...document.querySelectorAll(".history-message, .history-summary, .history-compare option")].map((e) => e.textContent).join("|")';
  check("no change is named by an edit command", !/[a-z]_[a-z]/.test(evaluate(NAMED)), evaluate(NAMED));
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-titles.png`);
  choose("Compare from", "main");
  choose("Compare to", "what-if");
  click(".history-compare button", "Compare");
  check("comparing with main shows the edit", waitFor(`${RESULT}.includes("A2") && ${RESULT}.includes("Original duration")`), evaluate(RESULT));
  click(".history-in-grid");
  check(
    "the grid shows Before and After",
    waitFor(`(${cellIs("A2", "After")} ?? "").includes("Original duration 6 days")`),
    [evaluate(cellIs("A2", "Before")), evaluate(cellIs("A2", "After"))],
  );
  click(".history-in-grid");
  check("and hides them again", waitFor('![...document.querySelectorAll("#grid-mirror th")].some((th) => th.textContent === "After")'));

  // A baseline from the newest entry; then an edit, compared with it.
  click(".history-entry", "A2");
  click("button", "Baseline from here");
  fill("Baseline's name", "agreed");
  click("button", "Create baseline");
  check("the baseline is listed", waitFor('[...document.querySelectorAll(".history-baselines li")].some((li) => li.textContent.includes("agreed"))'));
  const finishBefore = evaluate(cellIs("A3", "Original (d)"));
  edit("A3", "Original (d)", "5");
  press("Compare baseline agreed with now");
  check(
    "comparing the head with the baseline shows the later edit only",
    waitFor(`${RESULT}.includes("A3") && !${RESULT}.includes("A2")`),
    evaluate(RESULT),
  );

  // Take the A3 edit out.
  const changes = log().length;
  click(".history-entry", "A3");
  click("button", "Revert");
  check(
    "the revert prompt names it in words",
    waitFor('document.querySelector(".history-detail")?.textContent.includes("Taking out A3 duration records")'),
    evaluate('document.querySelector(".history-detail")?.textContent ?? null'),
  );
  fill("Why take it out", "Back to three days");
  click("button", "Take it out");
  check("the entry is taken out", waitFor(`${NOTICE}.startsWith("Took out ")`), evaluate(NOTICE));
  check("as a revert on what-if", log().length === changes + 1 && /\brevert\b/.test(log()[0]), log()[0]);
  check("the duration is back", waitFor(`${cellIs("A3", "Original (d)")} === ${JSON.stringify(finishBefore)}`), evaluate(cellIs("A3", "Original (d)")));

  // Export the import, and compare its bytes with the CLI's.
  evaluate(`(() => {
    window.__exported = null;
    const make = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (blob) => {
      blob.arrayBuffer().then((bytes) => { window.__exported = btoa(String.fromCharCode(...new Uint8Array(bytes))); });
      return make(blob);
    };
    return true;
  })()`);
  click(".history-entry", "Start");
  click(".history-detail button", "Export XER");
  check("the export downloads", waitFor("window.__exported !== null"), evaluate(NOTICE));
  const start = log().at(-1).slice(0, 10);
  check("named for the store and the change", evaluate(NOTICE) === `Exported plant-${start}.xer.`, evaluate(NOTICE));
  const exported = join(dir, "start.xer");
  totalfloat("history", "export", store, "--at", start, "-o", exported);
  check("its bytes match `history export`", evaluate("window.__exported") === readFileSync(exported).toString("base64"));

  // A log longer than the window: Compare with now brings the comparison, at
  // the panel's foot, into view.
  for (let i = 1; i <= 12; i++) totalfloat("edit", store, "--duration", `A4=${i + 2}d`, "-m", `Step ${i}`);
  check("the log lists the later changes", waitFor('[...document.querySelectorAll(".history-entry .history-message")].some((m) => m.textContent === "Step 12")'));
  const comparisonInView = `(() => {
    const line = document.querySelector(".history-result")?.firstElementChild;
    if (!line) return false;
    const { top, bottom } = line.getBoundingClientRect();
    for (let box = line.parentElement; box; box = box.parentElement) {
      if (!/auto|scroll/.test(getComputedStyle(box).overflowY)) continue;
      const shown = box.getBoundingClientRect();
      if (top < shown.top || bottom > shown.bottom) return false;
    }
    return top >= 0 && bottom <= innerHeight;
  })()`;
  click(".history-entry", "Step 11");
  press("Compare with now");
  check("a change's Compare with now shows the comparison", waitFor(`${RESULT}.includes("A4")`), evaluate(RESULT));
  check("in view, without scrolling", waitFor(comparisonInView, 3_000), evaluate('document.querySelector(".history-result")?.getBoundingClientRect().top ?? null'));
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-compare-in-view.png`);
  // Back to the top of the log, then a baseline's.
  evaluate('(document.querySelector(".history-entry").scrollIntoView({ block: "start" }), true)');
  check("the comparison is out of view at the log's top", !evaluate(comparisonInView));
  press("Compare baseline agreed with now");
  check("a baseline's Compare with now shows the comparison", waitFor(`${RESULT}.includes("A4")`), evaluate(RESULT));
  check("in view too", waitFor(comparisonInView, 3_000), evaluate('document.querySelector(".history-result")?.getBoundingClientRect().top ?? null'));

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
