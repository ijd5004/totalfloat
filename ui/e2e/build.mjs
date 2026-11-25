// Building a schedule in the grid, in a real browser, through playwright-cli,
// against a store made from fixtures/msproject/plant.xml:
//   - Civil's Add activity line opens a new row with the ID offered and the
//     name focused; three typed with Enter between them wait as three edits,
//     with IDs following the band's;
//   - Apply records them as one changeset, in order, the 0-day one a milestone;
//   - the History view lists the changeset with its author;
//   - Alt+Insert adds a WBS node under the project's, typed in place, which
//     shows as a band while it waits; three activities typed into it record
//     with it;
//   - F2 on its band renames it, and Alt+M moves Install fence into it;
//   - deleting one of its activities, then Ctrl+Z, puts it back;
//   - Delete on Excavate and Pour slab asks, with the counts of activities,
//     links and assignments; Apply deletes them, and Ctrl+Z puts them back
//     with their links and assignment;
//   - the file at the head exports, and with --validate passes
//     `cargo xtask validate xer`.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/build.mjs [--bin target/release/totalfloat] [--screenshot build] [--validate]
//
// --screenshot PREFIX writes PREFIX-adding-light.png, PREFIX-adding-dark.png,
// PREFIX-wbs-light.png, PREFIX-wbs-dark.png, PREFIX-move-light.png and
// PREFIX-delete-light.png. Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const validate = flags.includes("--validate");
const session = `build-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-build-e2e-"));
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

/** Screenshots the page as it is, in the light theme or the dark. */
function screenshot(name, scheme) {
  if (!shot) return;
  cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: "${scheme}" }); }`);
  pause(300);
  cli("screenshot", `--filename=${shot}-${name}-${scheme}.png`);
  cli("run-code", "async (page) => { await page.emulateMedia({ colorScheme: null }); }");
}

/** Each activity's ID and type in the store's checked-out file, as `query` lists them. */
function activities() {
  const [, ...lines] = totalfloat("query", store, "--columns", "code,type").trim().split("\n");
  return new Map(
    lines
      .filter((line) => !line.startsWith("Showing "))
      .map((line) => line.trim().split(/\s+/))
      .map(([code, type]) => [code, type]),
  );
}

/** Each activity's WBS code in the store's checked-out file, as `query` lists them. */
function wbsOf() {
  const [, ...lines] = totalfloat("query", store, "--columns", "code,wbs").trim().split("\n");
  return new Map(
    lines
      .filter((line) => !line.startsWith("Showing "))
      .map((line) => /^(\S+)\s+(.*)$/.exec(line.trim()) ?? [])
      .map(([, code, wbs]) => [code, wbs]),
  );
}

/** An expression for whether a band's mirror text starts with `text`. */
const band = (text) => `[...document.querySelectorAll("#grid-mirror tbody tr")].some((tr) => tr.cells.length === 1 && tr.textContent.startsWith(${JSON.stringify(text)}))`;

/** The changesets on main, newest first, as `history log` lists them: ID, time, author, kind, summary. */
function log() {
  return totalfloat("history", "log", store)
    .split("\n")
    .filter((line) => /^[0-9a-f]{10} /.test(line));
}

const NOTICE = 'document.querySelector(".edit-bar .notice")?.textContent ?? ""';
const PENDING = 'document.querySelector(".pending")?.textContent ?? ""';

/** The grid line whose mirror text starts with `text`, or -1. */
function lineOf(text) {
  return evaluate(`(() => {
    const tr = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.textContent.startsWith(${JSON.stringify(text)}));
    return tr ? Number(tr.getAttribute("aria-rowindex")) - 2 : -1;
  })()`);
}

/** An expression for whether the grid shows an activity, by its ID. */
const shows = (code) => `[...document.querySelectorAll("#grid-mirror tbody tr")].some((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)})`;

/** Puts the active cell on grid line `line`, from the keyboard. */
function goToLine(line) {
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  for (let i = 0; i < line; i++) cli("press", "ArrowDown");
}

function apply() {
  cli("run-code", 'async (page) => { await page.getByRole("group", { name: "Edits pending" }).getByRole("button", { name: "Apply" }).click(); }');
}

let server;
try {
  totalfloat("import", join(repo, "fixtures/msproject/plant.xml"), "-o", "plant.xer");
  totalfloat("history", "init", "plant.xer", "-m", "Start");
  const before = activities();

  server = spawn(bin, ["serve", `${store}@main`], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
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
  cli("resize", "1600", "950");
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));

  // Three activities typed into Civil's band, Enter between them.
  const civil = lineOf("Add an activity to Civil");
  check("Civil's band ends with an Add activity line", civil >= 0, civil);
  goToLine(civil);
  cli("press", "Enter");
  check("Enter opens a new row with the name focused", waitFor('document.activeElement?.getAttribute("aria-label") === "New activity\'s name"'));
  const offered = [];
  for (const [name, duration] of [["Survey", "2"], ["Pour footings", "3d"], ["Ready", "0"]]) {
    offered.push(evaluate('document.querySelector(\'[aria-label="New activity\\\'s ID"]\')?.value ?? null'));
    cli("type", name);
    cli("press", "Tab");
    cli("type", duration);
    cli("press", "Enter");
    waitFor(`(${PENDING}).startsWith("${offered.length} edit")`);
  }
  // Civil holds A2 and A3, stepping by 1; A5, A6 and A8 are other bands'.
  check("the IDs follow Civil's, skipping those taken", JSON.stringify(offered) === JSON.stringify(["A4", "A7", "A9"]), offered);
  check("each waits, with the next row open", evaluate(PENDING).startsWith("3 edits pending"), evaluate(PENDING));
  check("they show in Civil's band before they're recorded", evaluate(`${shows("A4")} && ${shows("A7")} && ${shows("A9")}`));
  screenshot("adding", "light");
  screenshot("adding", "dark");
  cli("press", "Escape");
  check("Escape closes the new row", waitFor('!document.querySelector(".new-row")'));

  let changesets = log().length;
  apply();
  check("Apply records them", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  const [added] = log();
  check("as one changeset", log().length === changesets + 1);
  check("in the order typed", / Added A4, Added A7, Added A9$/.test(added ?? ""), added);
  const after = activities();
  check("the 0-day one is a milestone", after.get("A9") === "finish-milestone" && after.get("A4") === "task", [after.get("A4"), after.get("A9")]);

  // The History view lists it with its author.
  const author = (added ?? "").split(/\s+/)[2];
  cli("press", "Alt+h");
  check(
    "History lists the changeset with its author",
    waitFor(`(document.querySelector(".history-log .history-entry .history-meta")?.textContent ?? "").startsWith(${JSON.stringify(`${author} · `)})`),
    evaluate('document.querySelector(".history-log .history-entry .history-meta")?.textContent ?? null'),
  );
  cli("press", "Alt+Shift+g");
  check("back to the schedule", waitFor('!document.querySelector("#view-history:not([hidden])")'));

  // A WBS node under the project's own, typed in place, with three activities in it.
  goToLine(0);
  cli("press", "Alt+Insert");
  check("Alt+Insert opens a new WBS node's line, its code focused", waitFor(`document.activeElement?.getAttribute("aria-label") === "New WBS node's code"`));
  cli("type", "3");
  cli("press", "Tab");
  cli("type", "Survey works");
  cli("press", "Enter");
  check("the node waits as a band of its own", waitFor(`(${PENDING}).startsWith("1 edit pending") && ${band("WBS Survey works")}`), evaluate(PENDING));
  goToLine(lineOf("Add an activity to Survey works"));
  cli("press", "Enter");
  const offeredHere = [];
  for (const [name, duration] of [["Walk the site", "1"], ["Set out", "2"], ["Survey done", "0"]]) {
    waitFor(`document.activeElement?.getAttribute("aria-label") === "New activity's name"`);
    offeredHere.push(evaluate('document.querySelector(\'[aria-label="New activity\\\'s ID"]\')?.value ?? null'));
    cli("type", name);
    cli("press", "Tab");
    cli("type", duration);
    cli("press", "Enter");
    waitFor(`(${PENDING}).startsWith("${offeredHere.length + 1} edits")`);
  }
  // The node has no IDs of its own, so they follow the schedule's, after Civil's new ones.
  check("its IDs follow the schedule's", JSON.stringify(offeredHere) === JSON.stringify(["A10", "A11", "A12"]), offeredHere);
  cli("press", "Escape");
  screenshot("wbs", "light");
  screenshot("wbs", "dark");
  changesets = log().length;
  apply();
  check("Apply records the node and its activities", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  const [built] = log();
  check("as one changeset, the node first", log().length === changesets + 1 && / Added WBS 3, Added A10, Added A11, and 1 more$/.test(built ?? ""), built);
  check("its activities are in it", ["A10", "A11", "A12"].every((code) => wbsOf().get(code) === "3"), Object.fromEntries(wbsOf()));

  // F2 on its band renames it; Alt+M moves Install fence into it.
  check("the recorded node shows", waitFor(band("WBS Survey works")));
  goToLine(lineOf("WBS Survey works"));
  cli("press", "F2");
  check("F2 opens its name", waitFor(`document.activeElement?.getAttribute("aria-label") === "WBS node's name"`));
  cli("press", "Control+a");
  cli("type", "Site survey");
  cli("press", "Enter");
  check("the new name shows while it waits", waitFor(band("WBS Site survey")));
  goToLine(lineOf("A5"));
  cli("press", "Alt+m");
  check("Alt+M asks which node", waitFor('!!document.querySelector(".wbs-picker")'));
  cli("type", "Site");
  screenshot("move", "light");
  cli("press", "Enter");
  check("the move waits", waitFor(`(${PENDING}).startsWith("2 edits pending")`), evaluate(PENDING));
  changesets = log().length;
  apply();
  check("Apply records the rename and the move", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  check("in one changeset", / Renamed WBS \S+, Moved A5 to WBS \S+$/.test(log()[0] ?? "") && log().length === changesets + 1, log()[0]);
  check("Install fence is in it", wbsOf().get("A5") === "3");
  check("under its new name", waitFor(`${band("WBS Site survey")} && !${band("WBS Survey works")}`));

  // One of its activities deleted, then put back.
  goToLine(lineOf("A11"));
  cli("press", "Delete");
  check("Delete asks about one activity", waitFor('document.querySelector(".delete-dialog h2")?.textContent === "Delete 1 activity, with 0 links and 0 assignments?"'));
  cli("run-code", 'async (page) => { await page.getByRole("alertdialog").getByRole("button", { name: "Delete", exact: true }).click(); }');
  waitFor(`(${PENDING}).startsWith("1 edit pending")`);
  apply();
  check("Apply deletes it", waitFor(`(${NOTICE}).startsWith("Recorded ") && !${shows("A11")}`), evaluate(NOTICE));
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+z");
  check("Ctrl+Z puts it back, in its node", waitFor(`(${NOTICE}).startsWith("Took out ") && ${shows("A11")}`) && wbsOf().get("A11") === "3", evaluate(NOTICE));

  // Delete A2 and A3, Excavate and Pour slab: asked first, with the counts.
  // Between them they have 2 links (A2 to A3, A3 to A5) and the one assignment.
  goToLine(lineOf("A2"));
  cli("press", "Shift+ArrowDown");
  cli("press", "Delete");
  check("Delete asks first", waitFor('!!document.querySelector(".delete-dialog")'));
  const asks = evaluate('document.querySelector(".delete-dialog h2")?.textContent ?? null');
  check("naming the activities, links and assignments", asks === "Delete 2 activities, with 2 links and 1 assignment?", asks);
  check("with Cancel focused", evaluate('document.activeElement?.textContent === "Cancel"'));
  screenshot("delete", "light");
  cli("run-code", 'async (page) => { await page.getByRole("alertdialog").getByRole("button", { name: "Delete", exact: true }).click(); }');
  check("the delete waits", waitFor(`(${PENDING}).startsWith("2 edits pending")`), evaluate(PENDING));
  check("and the rows leave the grid", waitFor(`!${shows("A2")} && !${shows("A3")} && ${shows("A4")}`));
  changesets = log().length;
  apply();
  check("Apply deletes them", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  check("from the file", !activities().has("A2") && !activities().has("A3") && log().length === changesets + 1);

  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+z");
  check("Ctrl+Z puts them back", waitFor(`(${NOTICE}).startsWith("Took out ") && ${shows("A2")} && ${shows("A3")}`), evaluate(NOTICE));
  const restored = activities();
  check("in the file", restored.has("A2") && restored.has("A3") && restored.size === before.size + 6, [before.size, restored.size]);

  // The file at the head exports, and passes the XER check.
  const exported = join(dir, "built.xer");
  totalfloat("history", "export", store, "-o", exported);
  const tables = readFileSync(exported, "utf8").split("%T\t").map((table) => [table.split("\n")[0].trim(), table.split("\n").filter((l) => l.startsWith("%R")).length]);
  const rowsOf = (name) => tables.find(([table]) => table === name)?.[1] ?? 0;
  check("the head exports, with its 4 links and 1 assignment back", rowsOf("TASKPRED") === 4 && rowsOf("TASKRSRC") === 1, [rowsOf("TASKPRED"), rowsOf("TASKRSRC")]);
  if (validate) {
    // On a quiet machine: XER Tools gets a fixed time under Wine to run its checks.
    cli("close");
    server.kill();
    let out = "";
    let ok = true;
    try {
      out = execFileSync("cargo", ["xtask", "validate", "xer", exported], { encoding: "utf8", cwd: repo, stdio: ["ignore", "pipe", "pipe"] });
    } catch (error) {
      ok = false;
      out = `${error.stdout ?? ""}${error.stderr ?? ""}`;
    }
    console.log(out.trim());
    check("it passes cargo xtask validate xer", ok);
  }
} catch (error) {
  console.error(error);
  check("ran", false);
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
