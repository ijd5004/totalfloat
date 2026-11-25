// The risk view, in a real browser, through playwright-cli: a seeded
// plan of 300 activities built from schedule text, served, the risk view
// opened from its shortcut (Alt+Shift+R), a simulation run with a range and
// a target, and the P50 the page shows checked against `totalfloat risk
// --json` for the same inputs and seed. Then an activity in the sensitivity
// list, and one in the criticality list, are clicked and must be selected in
// the grid.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/risk.mjs [--bin target/release/totalfloat] [--screenshot risk.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const session = `risk-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-risk-e2e-"));

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

/** Types into the field with this label. */
function fill(label, text) {
  cli("run-code", `async (page) => { await page.getByLabel(${JSON.stringify(label)}, { exact: true }).fill(${JSON.stringify(text)}); }`);
}

/**
 * A seeded plan: `n` activities in 40 parallel streams across ten WBS
 * nodes, each after the one before it in its stream and often after one
 * in a neighbouring stream, so the network is wide and deep.
 */
function plan(n) {
  let seed = 11;
  const next = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  };
  const STREAMS = 40;
  const lines = ["totalfloat-schedule 1", "", 'project NET "Network test"', "  data-date 2026-01-05 08:00", ""];
  const per = Math.ceil(n / 10);
  for (let w = 0; w < 10; w++) {
    lines.push(`wbs NET.W${w} "Area ${w}"`, "");
    for (let id = w * per + 1; id <= Math.min(n, (w + 1) * per); id++) {
      const before = new Set();
      if (id > STREAMS) before.add(id - STREAMS);
      if (id > STREAMS + 3 && next() < 0.5) before.add(id - STREAMS - 1 - Math.floor(next() * 3));
      if (id > STREAMS * 4 && next() < 0.1) before.add(id - STREAMS * (2 + Math.floor(next() * 3)));
      const after = before.size > 0 ? ` after ${[...before].sort((a, b) => a - b).map((b) => `A${b}`).join(", ")}` : "";
      lines.push(`activity A${id} "Activity ${id}" ${1 + Math.floor(next() * 9)}d${after}`);
    }
    lines.push("");
  }
  return lines.join("\n");
}

let server;
try {
  writeFileSync(join(dir, "plan.tfs"), plan(300));
  execFileSync(bin, ["import", join(dir, "plan.tfs"), "-o", join(dir, "plan.xer")], { cwd: dir, stdio: "inherit" });
  const cliReport = JSON.parse(
    execFileSync(
      bin,
      ["risk", join(dir, "plan.xer"), "--range=-10,40", "--iterations", "500", "--seed", "7", "--target", "2026-12-31 17:00", "--json"],
      { cwd: dir, encoding: "utf8" },
    ),
  );
  const cliP50 = cliReport.percentiles.find((p) => p.p === 50).finish;
  // As the page shows dates by default: 03 Apr 26.
  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const shown = (date) => `${date.slice(8, 10)} ${MONTHS[Number(date.slice(5, 7)) - 1]} ${date.slice(2, 4)}`;

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
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")', 60_000));

  // The risk view from its shortcut.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+R");
  check("the risk view opens", waitFor('!!document.querySelector("section[aria-label=\'Schedule risk\']")'));
  fill("Pessimistic %", "40");
  fill("Iterations", "500");
  fill("Seed", "7");
  fill("Finish by", "2026-12-31 17:00");
  cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Run", exact: true }).click(); }');
  const P50 = '[...document.querySelectorAll(".risk-p div")].find((d) => d.querySelector("dt")?.textContent === "P50")?.querySelector("dd")?.textContent ?? null';
  check("results arrive", waitFor(`(${P50}) !== null`, 60_000));
  check("the page's P50 is the command's for the same inputs and seed", evaluate(P50) === shown(cliP50), [evaluate(P50), cliP50]);
  const summary = evaluate('document.querySelector(".risk-summary")?.textContent ?? ""');
  check("the target's chance is shown", summary.includes("Finish by 31 Dec 26"), summary);
  check("the histogram is drawn", evaluate('document.querySelectorAll(".risk-histogram .risk-bar").length') > 0);
  check("the tornado is drawn", evaluate('document.querySelectorAll(".risk-tornado .risk-sensitivity").length') > 0);

  // What the page shows after a click: the view tab selected, the
  // risk view gone, and the selected activity with whether its row is inside
  // the grid's scrolled view (rows are the body's height over its lines).
  const SHOWN = `(() => {
    const grid = document.querySelector(".grid-body");
    const table = document.getElementById("grid-mirror");
    const tr = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.querySelector("[aria-selected=true]"));
    const lines = Number(table?.getAttribute("aria-rowcount") ?? 1) - 1;
    const row = grid && lines > 0 ? grid.scrollHeight / lines : 0;
    const top = tr ? (Number(tr.getAttribute("aria-rowindex")) - 2) * row - grid.scrollTop : -1;
    return {
      tab: document.querySelector("[role=tab][aria-selected=true]")?.textContent ?? null,
      risk: !!document.querySelector("section[aria-label='Schedule risk']"),
      selected: tr?.querySelector("td")?.textContent ?? null,
      inView: !!tr && top >= 0 && top + row <= grid.clientHeight,
    };
  })()`;
  const showsSchedule = (code) =>
    `(() => { const s = ${SHOWN}; return s.tab === "Schedule" && !s.risk && s.selected === ${JSON.stringify(code)} && s.inView; })()`;

  // An activity in the sensitivity list, clicked, is shown selected in the
  // schedule. The last one listed, so the grid has to scroll to it.
  const first = cliReport.sensitivity[Math.min(9, cliReport.sensitivity.length - 1)].code;
  cli("run-code", `async (page) => { await page.locator(".risk-list").nth(0).getByRole("button", { name: ${JSON.stringify(`${first} `)} }).first().click(); }`);
  check("a listed activity clicked shows the schedule with it selected and in view", waitFor(showsSchedule(first)), [evaluate(SHOWN), first]);

  // And one in the criticality list, the second list, after opening the
  // risk view again: its results are kept.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+R");
  check("the risk view opens again with its results", waitFor('document.querySelectorAll(".risk-list").length > 1'));
  const critical = cliReport.criticality.find((r) => r.code !== first)?.code;
  cli("run-code", `async (page) => { await page.locator(".risk-list").nth(1).getByRole("button", { name: ${JSON.stringify(`${critical} `)} }).first().click(); }`);
  check("a critical activity clicked shows the schedule with it selected and in view", waitFor(showsSchedule(critical)), [evaluate(SHOWN), critical]);

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
