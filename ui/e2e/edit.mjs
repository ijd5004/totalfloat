// Editing in the grid, in a real browser, through playwright-cli, against a
// store made from a synthetic fixture:
//   - a duration edit is previewed, recorded as a changeset, and moves the finish;
//   - the edits-waiting button's text reads in both themes (4.5:1);
//   - a bad span shows its error on the cell, and nothing is sent;
//   - a pasted three-cell range is one changeset;
//   - Ctrl+C and Ctrl+V, pressed as keys, copy and paste in the grid after the
//     filter box and a cell's input have had the focus;
//   - Ctrl+Z reverts it;
//   - an edit made by another process meanwhile gives the moved-branch banner,
//     and the typed value is kept and can be applied again;
//   - a reason typed for edits that were discarded isn't recorded with the next;
//   - a duration typed in the inspector sends the same edit the grid does,
//     waits in the grid's cell too, and once applied shows the new finish.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/edit.mjs [--bin target/release/totalfloat] [--screenshot edit.png]
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
const session = `edit-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-edit-e2e-"));
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

/**
 * An expression for the WCAG contrast ratio of the first `selector`'s text
 * against its own background, or null if it isn't on the page.
 */
function contrastOf(selector) {
  return `(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const style = getComputedStyle(el);
    const lum = (css) => {
      const [r, g, b] = css.match(/[\\d.]+/g).slice(0, 3).map((v) => {
        const c = Number(v) / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const [hi, lo] = [lum(style.color), lum(style.backgroundColor)].sort((a, b) => b - a);
    return Math.round(((hi + 0.05) / (lo + 0.05)) * 100) / 100;
  })()`;
}

/** Checks `selector`'s text reads (WCAG AA, 4.5:1) in the light and dark themes. */
function checkContrast(name, selector) {
  for (const scheme of ["light", "dark"]) {
    cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: "${scheme}" }); }`);
    pause(150);
    const ratio = evaluate(contrastOf(selector));
    check(`${name} reads in the ${scheme} theme (4.5:1)`, ratio !== null && ratio >= 4.5, ratio);
  }
  cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: null }); }`);
}

/** The changesets on main, newest first, as `history log` lists them. */
function log() {
  return totalfloat("history", "log", store)
    .split("\n")
    .filter((line) => /^[0-9a-f]{10} /.test(line));
}

const NOTICE = 'document.querySelector(".edit-bar .notice").textContent';

/** An expression for the mirror's text for an activity's cell, by its ID and the column's title. */
function cellIs(code, title) {
  return `(() => {
    const titles = [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const row = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    const td = row && row.cells[titles.indexOf(${JSON.stringify(title)})];
    return td ? { text: td.textContent, invalid: td.getAttribute("aria-invalid") === "true" } : null;
  })()`;
}

function cell(code, title) {
  return evaluate(cellIs(code, title));
}

/** Puts the active cell on an activity's cell, from the keyboard, the grid having the focus. */
function moveTo(code, title) {
  const [line, column] = evaluate(`(() => {
    const titles = [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const row = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    return [Number(row.getAttribute("aria-rowindex")) - 2, titles.indexOf(${JSON.stringify(title)})];
  })()`);
  cli("press", "Control+Home");
  for (let i = 0; i < line; i++) cli("press", "ArrowDown");
  for (let i = 0; i < column; i++) cli("press", "ArrowRight");
}

/** Focuses the grid, then puts the active cell on an activity's cell. */
function goTo(code, title) {
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  moveTo(code, title);
}

/** Types into the active cell and commits it with Enter. */
function typeIn(text) {
  for (const key of text) cli("press", key === " " ? "Space" : key);
  check(`the cell's input holds ${text}`, evaluate('document.querySelector(".cell-input")?.value ?? null') === text);
  cli("press", "Enter");
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
  cli("resize", "1400", "800");
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));

  // A duration: previewed, then recorded on a second Enter; the finish moves.
  const finishBefore = cell("A4", "Finish").text;
  let changesets = log().length;
  goTo("A2", "Original (d)");
  typeIn("5d");
  check("the commit is previewed", waitFor(`${NOTICE}.startsWith("Preview: finish")`), evaluate(NOTICE));
  checkContrast("the edits pending", ".edit-bar .pending");
  check("the preview records nothing", log().length === changesets);
  cli("press", "Enter");
  check("Enter records it", waitFor(`${NOTICE}.startsWith("Recorded ")`), evaluate(NOTICE));
  check("as one changeset", log().length === changesets + 1, log()[0]);
  // The grid shows days of A2's calendar.
  check("the duration shows as recorded", waitFor(`${cellIs("A2", "Original (d)")}?.text === "5"`), cell("A2", "Original (d)"));
  const finishAfter = cell("A4", "Finish").text;
  check("the finish date updates", finishAfter !== finishBefore, { before: finishBefore, after: finishAfter });

  // A bad span: marked on its cell at once; nothing is sent.
  changesets = log().length;
  goTo("A1", "Original (d)");
  typeIn("5x");
  check("a bad span marks its cell", waitFor('!!document.querySelector("#grid-mirror td[aria-invalid=true]")'), cell("A1", "Original (d)"));
  check("with its error in the tooltip", evaluate('document.querySelector(".cell-tip")?.textContent ?? ""').includes(`"5x" isn't a duration`));
  check("and in the status bar", evaluate(NOTICE).includes(`"5x" isn't a duration`), evaluate(NOTICE));
  check("nothing is recorded", log().length === changesets);
  cli("press", "Escape");
  check("Escape discards it", waitFor('!document.querySelector("#grid-mirror td[aria-invalid=true]")'));

  // A reason typed for edits that are then discarded isn't recorded with the next edit.
  const WHY = 'document.querySelector(".changes .edit-message")';
  const openChanges = () => {
    if (!evaluate(`!!${WHY}`)) cli("press", "Alt+c");
    return waitFor(`!!${WHY}`);
  };
  goTo("A3", "Original (d)");
  typeIn("5d");
  check("an edit to discard is previewed", waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  check("the Changes panel shows the Why box", openChanges());
  evaluate(`(${WHY}.focus(), true)`);
  for (const key of "keepme") cli("press", key);
  check("the Why box holds the reason", evaluate(`${WHY}.value`) === "keepme", evaluate(`${WHY}.value`));
  evaluate('([...document.querySelectorAll(".changes button")].find((b) => b.textContent === "Discard").click(), true)');
  check("Discard drops the edit", waitFor(`${NOTICE} === "Discarded."`), evaluate(NOTICE));
  changesets = log().length;
  goTo("A3", "Original (d)");
  typeIn("6d");
  check("the next edit is previewed", waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  check("the Changes panel shows it", openChanges());
  check("with the Why box empty", evaluate(`${WHY}.value`) === "", evaluate(`${WHY}.value`));
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-why-after-discard.png`);
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Enter");
  check("Ctrl+Enter records it", waitFor(`${NOTICE}.startsWith("Recorded ")`) && log().length === changesets + 1, log()[0]);
  check("without the discarded reason", !log()[0].includes("keepme") && / A3 duration$/.test(log()[0]), log()[0]);

  // A pasted range of three cells: one changeset. The range goes on the
  // clipboard from the filter box, and Ctrl+C and Ctrl+V are pressed as keys:
  // browsers send clipboard events where the page's selection is, Firefox to
  // <body>, so a paste dispatched on the grid itself would miss what a person
  // meets.
  const GRID_FOCUSED = 'document.activeElement?.classList.contains("grid-body") === true';
  const FILTER = 'document.querySelector(".grid-filter").value';
  // The copies the page itself made, as the grid makes them.
  evaluate('(window.__copied = [], window.addEventListener("copy", (event) => { if (event.defaultPrevented) window.__copied.push(event.clipboardData.getData("text/plain")); }), true)');
  changesets = log().length;
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "/");
  check("/ moves into the filter box", waitFor('document.activeElement?.classList.contains("grid-filter") === true'));
  cli("run-code", 'async (page) => { await page.getByLabel("Filter activities by ID or name", { exact: true }).fill("Excavate\\t3d\\t2d"); }');
  cli("press", "Control+a");
  cli("press", "Control+c");
  check("Ctrl+C in the filter box is the box's own", evaluate("window.__copied.length") === 0, evaluate("window.__copied"));
  cli("press", "Backspace");
  check("which clears it", waitFor(`${FILTER} === ""`), evaluate(FILTER));
  cli("press", "Control+v");
  check("and so is Ctrl+V", waitFor(`${FILTER} === "Excavate\\t3d\\t2d"`), evaluate(FILTER));
  cli("press", "Control+a");
  cli("press", "Backspace");
  cli("press", "Escape");
  check("Escape goes back to the grid, unfiltered", waitFor(`${GRID_FOCUSED} && ${FILTER} === "" && !!(${cellIs("A4", "Activity ID")})`));
  moveTo("A1", "Activity name");
  cli("press", "Control+v");
  check("the paste is previewed", waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  cli("press", "Enter");
  check("Enter records the paste", waitFor(`${NOTICE}.startsWith("Recorded ")`), evaluate(NOTICE));
  check("the pasted range is one changeset", log().length === changesets + 1, log().slice(0, 2));
  const pasted = log()[0].slice(0, 10);
  // A1 is on an eight-hour day.
  check(
    "all three cells changed",
    waitFor(`${cellIs("A1", "Activity name")}?.text === "Excavate"`) &&
      cell("A1", "Original (d)").text === "3" &&
      cell("A1", "Remaining (d)").text === "2",
    [cell("A1", "Activity name"), cell("A1", "Original (d)"), cell("A1", "Remaining (d)")],
  );

  // Ctrl+Z takes it out, and says which changeset.
  cli("press", "Control+z");
  check("Ctrl+Z reverts it", waitFor(`${NOTICE}.startsWith("Took out ${pasted}")`), evaluate(NOTICE));
  check("as a revert on main", /\brevert\b/.test(log()[0]) && log().length === changesets + 2, log()[0]);
  check("the name is back", waitFor(`${cellIs("A1", "Activity name")}?.text === "Synthetic activity 1"`), cell("A1", "Activity name"));

  // Ctrl+C and Ctrl+V as keys after a cell's input has been open: from A2's
  // duration to A3's.
  changesets = log().length;
  moveTo("A2", "Original (d)");
  cli("press", "F2");
  check("F2 opens the cell's input", waitFor('document.activeElement?.classList.contains("cell-input") === true'));
  cli("press", "9");
  cli("press", "Escape");
  check("Escape closes it, back in the grid", waitFor(`${GRID_FOCUSED} && !document.querySelector(".cell-input")`));
  const copiedFrom = cell("A2", "Original (d)").text;
  cli("press", "Control+c");
  check("Ctrl+C copies the active cell", waitFor("window.__copied.length === 1"), evaluate("window.__copied"));
  const copied = evaluate("window.__copied.at(-1) ?? null");
  check("as the cell takes it", copied !== null && parseFloat(copied) === parseFloat(copiedFrom), { copied, shown: copiedFrom });
  cli("press", "ArrowDown");
  cli("press", "Control+v");
  // The typed cell shows the pasted days with their unit.
  check(
    "Ctrl+V pastes it into A3, previewed",
    waitFor(`${NOTICE}.startsWith("Preview: ") && parseFloat(${cellIs("A3", "Original (d)")}?.text) === ${parseFloat(copied)}`),
    [evaluate(NOTICE), cell("A3", "Original (d)")],
  );
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-keys-pasted.png`);
  cli("press", "Enter");
  check("Enter records the pasted cell", waitFor(`${NOTICE}.startsWith("Recorded ")`) && log().length === changesets + 1, log()[0]);
  check("A3 takes A2's duration", waitFor(`${cellIs("A3", "Original (d)")}?.text === ${JSON.stringify(copiedFrom)}`), cell("A3", "Original (d)"));

  // Another writer moves the branch between the preview and applying it.
  goTo("A3", "Original (d)");
  typeIn("4d");
  check("the next edit is previewed", waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  totalfloat("edit", store, "--name", "A4=Renamed elsewhere", "-m", "Another writer");
  changesets = log().length;
  cli("press", "Enter");
  check("the page shows the moved-branch banner", waitFor('!!document.querySelector(".banner[role=alert]")'), evaluate(NOTICE));
  check("nothing of ours is recorded", log().length === changesets);
  check("the grid shows the other writer's change", waitFor(`${cellIs("A4", "Activity name")}?.text === "Renamed elsewhere"`));
  check("the typed value is kept", cell("A3", "Original (d)").text === "4d", cell("A3", "Original (d)"));
  evaluate('([...document.querySelectorAll(".banner button")].find((b) => b.textContent === "Apply again").click(), true)');
  check("Apply again previews it on their change", waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Enter");
  check("and records it", waitFor(`${NOTICE}.startsWith("Recorded ")`) && log().length === changesets + 1, log()[0]);

  // The inspector: the same flow from beside the Gantt.
  // Keeps each edit the page sends, to compare with the grid's. The app
  // holds its own reference to fetch, so the browser's requests are watched.
  evaluate("(window.__edits = [], true)");
  cli(
    "run-code",
    `async (page) => {
      page.on("request", (request) => {
        if (request.url().includes("/api/1/edit")) void page.evaluate((body) => window.__edits?.push(JSON.parse(body)), request.postData());
      });
    }`,
  );
  goTo("A4", "Activity ID");
  cli("press", "Alt+i");
  check("Alt+I moves into the inspector, on A4", waitFor('document.activeElement?.classList.contains("inspector") && document.querySelector(".inspector-code")?.textContent === "A4"'));
  const field = (label) => `[...document.querySelectorAll(".inspector-field")].find((f) => f.querySelector("dt").textContent === ${JSON.stringify(label)})?.querySelector("dd")`;
  const finishBeforeInspector = evaluate(`${field("Finish")}.textContent`);
  changesets = log().length;
  evaluate(`(${field("Original duration")}.querySelector("button").click(), true)`);
  check("its original duration opens for typing", waitFor('document.activeElement?.classList.contains("inspector-input")'));
  cli("press", "Control+a");
  for (const key of "6d") cli("press", key);
  cli("press", "Enter");
  check("the inspector's edit is previewed", waitFor(`${NOTICE}.startsWith("Preview: ")`), evaluate(NOTICE));
  check(
    "as the grid's edit is: set_duration on A4, a dry run with the checks",
    evaluate('JSON.stringify(window.__edits.at(-1)?.edits) === JSON.stringify([{ op: "set_duration", activity: "A4", original: "6d" }]) && window.__edits.at(-1).dry_run === true && window.__edits.at(-1).check === true'),
    evaluate("window.__edits.at(-1) ?? null"),
  );
  check("the typed value waits in the grid's cell too", cell("A4", "Original (d)").text === "6d", cell("A4", "Original (d)"));
  check("and in the inspector, marked", evaluate(`${field("Original duration")}.querySelector("button.typed")?.textContent === "6d"`));
  check("nothing is recorded yet", log().length === changesets);
  if (shot) cli("screenshot", `--filename=${shot.replace(/\.png$/, "")}-inspector-pending.png`);
  cli("press", "Control+Enter");
  check("Ctrl+Enter records it", waitFor(`${NOTICE}.startsWith("Recorded ")`) && log().length === changesets + 1, log()[0]);
  check(
    "the inspector shows the new duration and finish",
    waitFor(`${field("Original duration")}.textContent === "6d" && ${field("Finish")}.textContent !== ${JSON.stringify(finishBeforeInspector)}`),
    { before: finishBeforeInspector, after: evaluate(`${field("Finish")}.textContent`) },
  );
  check("matching the grid's finish", evaluate(`${field("Finish")}.textContent`) === cell("A4", "Finish").text, cell("A4", "Finish"));

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
