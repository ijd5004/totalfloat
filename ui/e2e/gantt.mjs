// Logic lines and the driving path in a real browser, through playwright-cli,
// against a store made from a synthetic fixture. The path the page highlights
// must be exactly the chain the engine gives (`totalfloat explain`).
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/gantt.mjs [--bin target/release/totalfloat] [--screenshot gantt.png]
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
const shot = flag("--screenshot") ? resolve(flag("--screenshot")) : null;
const session = `gantt-e2e-${process.pid}`;
const dir = mkdtempSync(join(tmpdir(), "totalfloat-gantt-e2e-"));
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

function waitFor(expression, ms = 15_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (evaluate(expression)) return true;
    execFileSync(process.execPath, ["-e", "setTimeout(() => {}, 150)"]);
  }
  return false;
}

/** The activity IDs the grid lists now. */
const listed = () =>
  evaluate('[...document.querySelectorAll("#grid-mirror tbody tr")].filter((tr) => tr.cells.length > 1).map((tr) => tr.cells[0].textContent)');

let server;
try {
  // Two routes into A5: through A3 and A4, which take different times, and a
  // side branch, A6, that drives nothing.
  copyFileSync(join(repo, "fixtures/xer/batches/001/c-start-after.xer"), join(dir, "plant.xer"));
  totalfloat("history", "init", "plant.xer", "-m", "Start");
  const wbs = totalfloat("query", store, "--columns", "code,wbs").split("\n")[1].split(/\s+/)[1];
  totalfloat(
    "edit", store,
    "--add", `A3;Synthetic activity 3;${wbs};3d`,
    "--add", `A4;Synthetic activity 4;${wbs};1d`,
    "--add", `A5;Synthetic activity 5;${wbs};2d`,
    "--add", `A6;Synthetic activity 6;${wbs};1d`,
    "--link", "A2>A3", "--link", "A1>A4", "--link", "A3>A5", "--link", "A4>A5", "--link", "A1>A6",
    "-m", "Network",
  );
  // The engine's driving chain back from A5's early start.
  const explained = JSON.parse(totalfloat("explain", store, "A5", "--json"));
  const expected = [...new Set(explained.chains.flatMap((chain) => chain.steps.map((step) => step.code)))].sort();
  check("the engine gives a driving chain", expected.length > 1 && expected.includes("A5"), expected);

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
  cli("resize", "1500", "800");
  check("the grid and Gantt load", waitFor('(document.querySelector(".gantt-canvas")?.width ?? 0) > 300'));

  // It opens fitted: the first start and the last finish the engine gives are both on screen.
  const dates = JSON.parse(totalfloat("query", store, "--columns", "code,early_start,early_finish,actual_start,actual_finish", "--json"));
  const minutes = (text) => Date.UTC(+text.slice(0, 4), +text.slice(5, 7) - 1, +text.slice(8, 10), +text.slice(11, 13), +text.slice(14, 16)) / 60_000;
  const spans = dates.rows.map((r) => [minutes(r.actual_start ?? r.early_start), minutes(r.actual_finish ?? r.early_finish), r.code]);
  const onScreen = (from, to) =>
    evaluate(`(() => {
      const b = document.querySelector(".gantt-body");
      const x = (m) => ((m - Number(b.dataset.origin)) * Number(b.dataset.pxPerDay)) / 1440 - b.scrollLeft;
      return { zoom: b.dataset.zoom, from: Math.round(x(${from})), to: Math.round(x(${to})), width: b.clientWidth };
    })()`);
  const whole = onScreen(Math.min(...spans.map((s) => s[0])), Math.max(...spans.map((s) => s[1])));
  check("the Gantt opens fitted, first start to last finish on screen", whole.zoom === "fit" && whole.from >= 0 && whole.to <= whole.width && whole.to - whole.from > whole.width * 0.8, whole);
  // Filtered to one activity, Fit fits that activity's bar.
  const [a5start, a5finish] = spans.find((s) => s[2] === "A5");
  const before = onScreen(a5start, a5finish);
  evaluate('(document.querySelector(".grid-filter").focus(), true)');
  cli("type", "A5");
  waitFor('document.querySelector(".grid-count").textContent.startsWith("1 of")');
  const one = onScreen(a5start, a5finish);
  // A two-day bar between margins of a day: wider than it was among all six, and on screen.
  check("filtered, it fits the rows shown", one.from >= 0 && one.to <= one.width && one.to - one.from > 2 * (before.to - before.from), { before, one });
  evaluate('(document.querySelector(".grid-filter").focus(), true)');
  cli("press", "Control+a");
  cli("press", "Backspace");
  check("and the filter clears", waitFor('document.querySelector(".grid-count").textContent.startsWith("6 of")'));

  // Select A5 by clicking its bar's row in the Gantt, which also focuses the chart.
  const line = evaluate(
    '(() => { const tr = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells[0].textContent === "A5"); return Number(tr.getAttribute("aria-rowindex")) - 2; })()',
  );
  evaluate(`(() => {
    const body = document.querySelector(".gantt-body");
    const box = body.getBoundingClientRect();
    const grid = document.querySelector(".grid-body");
    const y = box.top + ${line} * 22 - grid.scrollTop + 11;
    body.focus();
    body.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: box.left + 5, clientY: y }));
    return true;
  })()`);
  check(
    "clicking a bar selects its activity",
    waitFor(`document.querySelector(".grid-body").getAttribute("aria-activedescendant")?.startsWith("grid-cell-${line}-")`),
  );

  // Alt+Shift+D shows the driving path; the grid filters to it to read it back.
  cli("press", "Alt+Shift+D");
  check("Alt+Shift+D highlights a driving path", waitFor('[...document.querySelectorAll(".grid-tools button")].some((b) => b.textContent === "Driving path")'));
  evaluate('([...document.querySelectorAll(".grid-tools button")].find((b) => b.textContent === "Driving path").click(), true)');
  waitFor(`document.querySelector(".grid-count").textContent.startsWith("${expected.length} of")`);
  const highlighted = listed().sort();
  check("exactly the engine's chain is highlighted", JSON.stringify(highlighted) === JSON.stringify(expected), { highlighted, expected });
  if (shot) cli("screenshot", `--filename=${shot}`);

  // A second Alt+Shift+D, from the grid this time, clears it, and the grid shows everything again.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+D");
  check("a second Alt+Shift+D, from the grid, clears it", waitFor('![...document.querySelectorAll(".grid-tools button")].some((b) => b.textContent === "Driving path")'));
  check("and the grid lists every activity again", waitFor('document.querySelector(".grid-count").textContent.startsWith("6 of")'));
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
