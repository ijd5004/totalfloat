// Building logic in a real browser, through playwright-cli, against a store
// made from fixtures/msproject/plant.xml:
//   - typing part of a name in the inspector's Add predecessor (Alt+B) lists
//     the match, and Enter adds an FS link, previewed with the finish it
//     moves, then recorded;
//   - changing that link's type to SS and its lag to 2d sends one change_link;
//     removing it sends one remove_link;
//   - dragging from one bar's finish to another's start adds FS, and from a
//     start to a start adds SS, the tooltip naming each before release;
//   - a link that closes a loop is refused, naming the loop's activities;
//   - each change is a changeset in the history, by a person.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/logic.mjs [--bin target/release/totalfloat] [--screenshots DIR]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const flag = (name) => {
  const at = flags.indexOf(name);
  return at >= 0 ? flags[at + 1] : undefined;
};
const bin = resolve(flag("--bin") ?? join(repo, "target/release/totalfloat"));
const shots = flag("--screenshots") ? resolve(flag("--screenshots")) : null;
const session = `logic-e2e-${process.pid}`;
const dir = mkdtempSync(join(tmpdir(), "totalfloat-logic-e2e-"));
const store = join(dir, "plant.tfdb");

const totalfloat = (...args) => execFileSync(bin, args, { encoding: "utf8", cwd: dir });
const cli = (...args) => execFileSync("playwright-cli", [`-s=${session}`, ...args], { encoding: "utf8" });

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

const pause = (ms) => execFileSync(process.execPath, ["-e", `setTimeout(() => {}, ${ms})`]);
function waitFor(expression, ms = 15_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (evaluate(expression)) return true;
    pause(150);
  }
  return false;
}

/** Screenshots in both themes, when asked for. */
function shoot(name) {
  if (!shots) return;
  for (const scheme of ["light", "dark"]) {
    cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: "${scheme}" }); }`);
    pause(200);
    cli("screenshot", `--filename=${join(shots, `${name}-${scheme}.png`)}`);
  }
  cli("run-code", "async (page) => { await page.emulateMedia({ colorScheme: null }); }");
}

/** The store's links, as `predecessor>successor type lag` by Activity ID, from the server's own answer. */
const LINKS = `(async () => {
  const get = (path) => fetch(path, { headers: { "x-totalfloat-session": localStorage.getItem("totalfloat-session") ?? "" } }).then((r) => r.json());
  const [links, rows] = await Promise.all([get("/api/1/links"), get("/api/1/rows")]);
  const code = (id) => rows.code[rows.id.indexOf(id)];
  return links.id.map((_, k) => code(links.predecessor[k]) + ">" + code(links.successor[k]) + " " + links.type[k] + " " + links.lag_hours[k]).sort();
})()`;

const NOTICE = 'document.querySelector(".edit-bar .notice")?.textContent ?? ""';
const changesets = () => JSON.parse(totalfloat("history", "log", store, "--json")).changesets;

/** Selects an activity from the grid's keyboard. */
function select(code) {
  const line = evaluate(
    `Number([...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)}).getAttribute("aria-rowindex")) - 2`,
  );
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  for (let i = 0; i < line; i++) cli("press", "ArrowDown");
  check(
    `${code} is selected`,
    waitFor(`document.getElementById(document.querySelector(".grid-body").getAttribute("aria-activedescendant"))?.closest("tr")?.cells[0]?.textContent === ${JSON.stringify(code)}`),
  );
}

/** Previews what's waiting, then records it with Ctrl+Enter; says whether one changeset was added. */
function applied(what) {
  const before = changesets().length;
  check(`${what} is previewed`, waitFor(`(${NOTICE}).startsWith("Preview: ")`), evaluate(NOTICE));
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Enter");
  check(`${what} is recorded as one changeset`, waitFor(`(${NOTICE}).startsWith("Recorded ")`) && changesets().length === before + 1, evaluate(NOTICE));
}

/** Where an end of an activity's bar is, in the page: its row's middle and its start or finish, from the chart's scale. */
function barEnd(code, end) {
  return evaluate(`(async () => {
    const get = (path) => fetch(path, { headers: { "x-totalfloat-session": localStorage.getItem("totalfloat-session") ?? "" } }).then((r) => r.json());
    const rows = await get("/api/1/rows");
    const i = rows.code.indexOf(${JSON.stringify(code)});
    const text = ${JSON.stringify(end)} === "start" ? (rows.actual_start[i] ?? rows.early_start[i]) : (rows.actual_finish[i] ?? rows.early_finish[i]);
    const m = Date.UTC(+text.slice(0, 4), +text.slice(5, 7) - 1, +text.slice(8, 10), +text.slice(11, 13), +text.slice(14, 16)) / 60000;
    const b = document.querySelector(".gantt-body");
    const box = b.getBoundingClientRect();
    const tr = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    const line = Number(tr.getAttribute("aria-rowindex")) - 2;
    const x = ((m - Number(b.dataset.origin)) * Number(b.dataset.pxPerDay)) / 1440 - b.scrollLeft;
    // A milestone's diamond: its left half is its start, its right half its finish.
    const nudge = rows.original_duration_hours[i] === 0 ? (${JSON.stringify(end)} === "start" ? -3 : 3) : 0;
    return { x: box.left + x + nudge, y: box.top + line * 22 - document.querySelector(".grid-body").scrollTop + 11 };
  })()`);
}

/**
 * Drags from one bar end to another, returning the tooltip as it read before
 * release; with `cancel`, presses Escape instead of releasing. `shot` names
 * screenshots taken before release.
 */
function drag(from, to, { cancel = false, shot = null } = {}) {
  const a = barEnd(...from);
  const b = barEnd(...to);
  cli("run-code", `async (page) => { await page.mouse.move(${a.x}, ${a.y}); await page.mouse.down(); await page.mouse.move(${(a.x + b.x) / 2}, ${(a.y + b.y) / 2}, { steps: 4 }); await page.mouse.move(${b.x}, ${b.y}, { steps: 4 }); }`);
  const tip = evaluate('document.querySelector(".gantt-drag-tip")?.textContent ?? null');
  const lit = evaluate('!!document.querySelector(".gantt-drag rect")');
  if (shot) shoot(shot);
  if (cancel) cli("press", "Escape");
  cli("run-code", "async (page) => { await page.mouse.up(); }");
  return { tip, lit };
}

let server;
try {
  copyFileSync(join(repo, "fixtures/msproject/plant.xml"), join(dir, "plant.xml"));
  totalfloat("import", "plant.xml", "-o", "plant.xer");
  totalfloat("history", "init", "plant.xer", "-m", "As planned");

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
  check("the grid and Gantt load", waitFor('(document.querySelector(".gantt-canvas")?.width ?? 0) > 300'));
  const start = evaluate(LINKS);
  check("plant's links to start with", JSON.stringify(start) === JSON.stringify(["A2>A3 FS 8", "A3>A5 SS 1.3333333333333333", "A5>A6 FF 0", "A6>A8 FS 0"]), start);
  // Keeps each edit request, to count the link edits in it.
  evaluate("(window.__edits = [], true)");
  cli(
    "run-code",
    `async (page) => {
      page.on("request", (request) => {
        if (request.url().includes("/api/1/edit")) void page.evaluate((body) => window.__edits?.push(JSON.parse(body)), request.postData());
      });
    }`,
  );
  const lastEdits = () => evaluate("window.__edits.at(-1)?.edits ?? null");

  // Typing: A6 gets A3 as a predecessor, found by part of its name, which moves the finish.
  select("A6");
  cli("press", "Alt+b");
  check("Alt+B moves into Add predecessor", waitFor('document.activeElement?.id === "inspector-add-predecessor"'));
  cli("type", "pour");
  check(
    "typing part of a name lists the match, with its finish",
    waitFor('[...document.querySelectorAll(".inspector-option")].map((o) => o.textContent).join("|") === "A3Pour slabends 18 Mar 26"'),
    evaluate('[...document.querySelectorAll(".inspector-option")].map((o) => o.textContent)'),
  );
  shoot("logic-typing");
  cli("press", "Enter");
  check("Enter sends one add_link, FS", waitFor('JSON.stringify(window.__edits.at(-1)?.edits) === JSON.stringify([{ op: "add_link", predecessor: "A3", successor: "A6", link_type: "FS" }])'), lastEdits());
  check("the preview shows the finish it moves", waitFor(`/^Preview: finish .* → /.test(${NOTICE})`), evaluate(NOTICE));
  check("and the title block's finish shows the slip", evaluate('document.querySelector(".slip")?.textContent ?? ""').startsWith("+"), evaluate('document.querySelector(".slip")?.textContent ?? null'));
  check("it waits in the inspector", evaluate('!!document.querySelector(".inspector-link-row.adding")'));
  shoot("logic-adding");
  applied("the typed link");
  check("the server has A3>A6 FS", waitFor(`${LINKS}.then((l) => l.includes("A3>A6 FS 0"))`), evaluate(LINKS));

  // Retyped: SS with 2d of lag, in one change_link.
  check("the inspector lists A3 as a predecessor", waitFor('!!document.querySelector("[aria-label=\'Type of the link with A3\']")'));
  cli("run-code", `async (page) => { await page.selectOption("[aria-label='Type of the link with A3']", "SS"); }`);
  evaluate('(document.querySelector("[aria-label=\'Lag of the link with A3\']").focus(), true)');
  cli("press", "Control+a");
  cli("type", "2d");
  cli("press", "Enter");
  check(
    "a new type and lag send one change_link",
    waitFor('JSON.stringify(window.__edits.at(-1)?.edits) === JSON.stringify([{ op: "change_link", predecessor: "A3", successor: "A6", link_type: "FS", new_type: "SS", lag: "2d" }])'),
    lastEdits(),
  );
  shoot("logic-retyped");
  applied("the retyped link");
  check("the server has A3>A6 SS with lag", waitFor(`${LINKS}.then((l) => l.some((x) => x.startsWith("A3>A6 SS ") && !x.endsWith(" 0")))`), evaluate(LINKS));

  // Removed: one remove_link.
  check("the link shows its new type", waitFor('document.querySelector("[aria-label=\'Type of the link with A3\']")?.value === "SS"'));
  evaluate('(document.querySelector("[aria-label=\'Remove the link with A3\']").click(), true)');
  check("Remove sends one remove_link", waitFor('JSON.stringify(window.__edits.at(-1)?.edits) === JSON.stringify([{ op: "remove_link", predecessor: "A3", successor: "A6", link_type: "SS" }])'), lastEdits());
  applied("the removal");
  check("the server no longer has A3>A6", waitFor(`${LINKS}.then((l) => !l.some((x) => x.startsWith("A3>A6")))`), evaluate(LINKS));

  // Dragging: A2's finish to A6's start is FS; A2's start to A5's start is SS.
  const fs = drag(["A2", "finish"], ["A6", "start"], { shot: "logic-drag" });
  check("the tooltip names the link before release", fs.tip?.startsWith("Link A2 to A6, finish to start") ?? false, fs.tip);
  check("and the target bar is outlined", fs.lit);
  check("release sends one add_link, FS", waitFor('JSON.stringify(window.__edits.at(-1)?.edits) === JSON.stringify([{ op: "add_link", predecessor: "A2", successor: "A6", link_type: "FS" }])'), lastEdits());
  applied("the dragged FS link");
  const ss = drag(["A2", "start"], ["A5", "start"]);
  check("start to start names SS before release", ss.tip?.startsWith("Link A2 to A5, start to start") ?? false, ss.tip);
  check("release sends one add_link, SS", waitFor('JSON.stringify(window.__edits.at(-1)?.edits) === JSON.stringify([{ op: "add_link", predecessor: "A2", successor: "A5", link_type: "SS" }])'), lastEdits());
  applied("the dragged SS link");
  const now = evaluate(LINKS);
  check("the server has both dragged links", now.includes("A2>A6 FS 0") && now.includes("A2>A5 SS 0"), now);

  // Escape cancels a drag: nothing is sent.
  const sent = evaluate("window.__edits.length");
  const cancelled = drag(["A5", "finish"], ["A8", "start"], { cancel: true });
  pause(300);
  check("Escape cancels a drag, sending nothing", cancelled.tip !== null && evaluate("window.__edits.length") === sent && !evaluate('!!document.querySelector(".gantt-drag")'), cancelled.tip);

  // A click on a bar's end selects it, and moving on afterwards drags nothing.
  const end = barEnd("A5", "finish");
  const other = barEnd("A8", "start");
  cli("run-code", `async (page) => { await page.mouse.click(${end.x}, ${end.y}); await page.mouse.move(${other.x}, ${other.y}, { steps: 6 }); }`);
  check("a click on a bar's end selects it", waitFor('document.querySelector(".inspector-code")?.textContent === "A5"'), evaluate('document.querySelector(".inspector-code")?.textContent ?? null'));
  check("and moving on after it drags nothing", !evaluate('!!document.querySelector(".gantt-drag")'));
  cli("run-code", `async (page) => { await page.mouse.click(${other.x}, ${other.y}); }`);
  pause(300);
  check("nor does the next click link anything", evaluate("window.__edits.length") === sent);

  // A loop: A8's finish back to A2's start, which A2 leads to.
  const loop = drag(["A8", "finish"], ["A2", "start"]);
  check("the loop's tooltip names it", loop.tip?.startsWith("Link A8 to A2, finish to start") ?? false, loop.tip);
  check(
    "a link that closes a loop is refused, naming the loop",
    waitFor(`/refused: .*loop.*A2.*A8|refused: .*loop.*A8.*A2/.test(${NOTICE})`),
    evaluate(NOTICE),
  );
  shoot("logic-loop");
  const loopsBefore = changesets().length;
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Escape");
  check("nothing of the loop is recorded", changesets().length === loopsBefore && !evaluate(LINKS).some((x) => x.startsWith("A8>A2")));

  // Each change is a changeset by a person, in the History view too.
  const log = changesets().slice(0, 5);
  check(
    "the five changes are changesets by a person",
    log.every((c) => c.author_kind === "human" && c.kind === "edit") && log.map((c) => c.summary).join("|").includes("Added link"),
    log.map((c) => `${c.short_id} ${c.author}/${c.author_kind} ${c.summary}`),
  );
  cli("press", "Alt+h");
  check(
    "the History view lists the newest, with its author",
    waitFor(`[...document.querySelectorAll(".history-entry")].some((e) => e.textContent.includes(${JSON.stringify(log[0].short_id)}) && e.textContent.includes(${JSON.stringify(log[0].author)}))`),
    evaluate('[...document.querySelectorAll(".history-entry")].slice(0, 2).map((e) => e.textContent)'),
  );
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
