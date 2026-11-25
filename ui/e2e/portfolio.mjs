// The Projects view, in a real browser, through playwright-cli: two
// schedules put in a portfolio with `portfolio new`, the portfolio served,
// the Projects view opened from its shortcut (Alt+Shift+E), its EPS checked
// against `portfolio show --json`, the shown project's figures checked
// against the title block (rel-fs-positive stores no dates, and the
// panel shows it calculated, as the grid does), the capacity chart drawn, and the other
// schedule opened from the tree, which the grid then shows.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/portfolio.mjs [--bin target/release/totalfloat] [--screenshot projects.png]
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
const session = `portfolio-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-portfolio-e2e-"));

function cli(...args) {
  return execFileSync("playwright-cli", [`-s=${session}`, ...args], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
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


let server;
try {
  copyFileSync(join(repo, "fixtures/xer/batches/001/rel-fs-positive.xer"), join(dir, "plant.xer"));
  copyFileSync(join(repo, "fixtures/xer/usage.xer"), join(dir, "usage.xer"));
  execFileSync(bin, ["portfolio", "new", "plant.xer", "usage.xer", "-o", "plan.portfolio.toml"], { cwd: dir, stdio: "inherit" });
  const shown = JSON.parse(execFileSync(bin, ["portfolio", "show", "plan.portfolio.toml", "--json"], { cwd: dir, encoding: "utf8" }));

  server = spawn(bin, ["serve", join(dir, "plan.portfolio.toml")], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
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
  check("the first schedule's grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")', 60_000));

  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+E");
  check("the project picker opens on the Projects panel", waitFor('!!document.querySelector("section[aria-label=Projects] .eps")', 30_000));
  const nodes = evaluate('document.querySelectorAll("section[aria-label=Projects] .eps-node").length');
  check("every EPS node is shown", nodes === shown.nodes.length, [nodes, shown.nodes.length]);
  const root = shown.nodes[0];
  const text = evaluate('document.querySelector("section[aria-label=Projects] .eps-node .eps-figures")?.textContent ?? ""');
  check("the top node's roll-up is the command's", text.startsWith(`${root.totals.activities} activities`), [text, root.totals.activities]);
  // The title block's finish is the calculated one; the panel's line for the
  // same project must end there and count both activities critical.
  const plant = shown.projects.find((p) => p.file === "plant.xer");
  const finish = evaluate('[...document.querySelectorAll(".title-block .fact")].find((f) => f.querySelector(".fact-label")?.textContent === "Finish")?.querySelector(".fact-value")?.textContent ?? ""');
  const line = evaluate(`document.querySelector(${JSON.stringify(`[aria-label="Open ${plant.code} in plant.xer"]`)})?.parentElement?.textContent ?? ""`);
  check("the title block shows the calculated finish", finish === "13 Jan 26", finish);
  check("the panel's line for the shown project is the grid's", line.includes(`12 Jan 26 to ${finish}, 2 critical`), line);
  check("and isn't labelled as stored", !line.includes("stored results"), line);
  check("the capacity chart is drawn", waitFor('document.querySelectorAll(".capacity-chart .capacity-bar").length > 0', 30_000));
  // By month the crew's 160 hours fit its 176; by week its 60-hour weeks don't fit 40.
  check("by month, nothing is over", evaluate('document.querySelectorAll(".capacity-chart .capacity-bar.over").length') === 0);
  cli("run-code", 'async (page) => { await page.getByLabel("By", { exact: true }).selectOption("week"); }');
  check("by week, the over-allocated weeks are marked", waitFor('document.querySelectorAll(".capacity-chart .capacity-bar.over").length === 2', 30_000), evaluate('document.querySelectorAll(".capacity-chart .capacity-bar.over").length'));

  // Open the usage schedule from the tree: the grid shows its activities.
  const other = shown.projects.find((p) => p.file === "usage.xer");
  cli("run-code", `async (page) => { await page.getByRole("button", { name: ${JSON.stringify(`Open ${other.code} in usage.xer`)} }).click(); }`);
  const GRID = '[...document.querySelectorAll("#grid-mirror tbody tr")].map((tr) => tr.querySelector("td")?.textContent).join(",")';
  check("the opened schedule is shown in the grid", waitFor('[...document.querySelectorAll("#grid-mirror tbody tr td")].some((td) => td.textContent === "A2")', 30_000), evaluate(GRID));
  check("the picker closes once a project is chosen", waitFor('!document.querySelector(".project-popup")'));
  cli("press", "Alt+Shift+E");
  check("and opened again, marks it as shown in the tree", waitFor('document.querySelector("[aria-current=true].eps-project")?.textContent === ' + JSON.stringify(other.code), 30_000));

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
