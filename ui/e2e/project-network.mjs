// The whole project's activity network, in a real browser, through
// playwright-cli: a seeded plan of 2,000 activities built from schedule
// text, served, the network shown in place of the Gantt (Alt+Shift+W),
// scrolled down and across a frame at a time with every gap between frames
// timed in the page, and a box clicked to select its activity in the grid.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/project-network.mjs [--bin target/release/totalfloat] [--screenshot network.png]
//
// Exits non-zero if a check fails, or a frame takes more than 50 ms.
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
const session = `project-network-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-project-network-e2e-"));

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

const FRAMES = `(step, n) => new Promise((done) => {
  const gaps = [];
  let last = 0;
  let count = 0;
  const tick = (now) => {
    if (last) gaps.push(now - last);
    last = now;
    if (count++ >= n || step() === false) return done(gaps);
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
})`;

const SCROLLER = 'document.querySelector(".project-network-scroll")';
const COUNT = 'document.querySelector(".view-network .network-count")?.textContent ?? null';
const BOXES = 'document.querySelectorAll(".project-network .network-box").length';

let server;
try {
  writeFileSync(join(dir, "plan.tfs"), plan(2000));
  execFileSync(bin, ["import", join(dir, "plan.tfs"), "-o", join(dir, "plan.xer")], { cwd: dir, stdio: "inherit" });

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
  check("2,000 activities", waitFor('document.querySelector(".grid-count")?.textContent === "2,000 of 2,000"', 60_000), evaluate('document.querySelector(".grid-count")?.textContent'));

  // The Network view, over the whole work area, from its key.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+W");
  check(
    "the Network view shows",
    waitFor(`!!${SCROLLER} && !!document.querySelector(".view-page.view-network") && document.querySelector("[role=tab][aria-selected=true]")?.textContent === "Network"`),
    evaluate(COUNT),
  );
  check("of every activity, in columns by logic", waitFor(`/^2,000 activities in \\d+ columns by logic$/.test(${COUNT})`), evaluate(COUNT));
  const drawn = evaluate(BOXES);
  check("only the boxes near the view are drawn", drawn > 0 && drawn < 2000, drawn);
  const size = evaluate(`(() => { const s = ${SCROLLER}; return { width: s.scrollWidth, height: s.scrollHeight }; })()`);

  // Scroll down and across, a step a frame, timing every frame.
  const down = evaluate(`(${FRAMES})(() => { const s = ${SCROLLER}; s.scrollTop += 120; return s.scrollTop + s.clientHeight < s.scrollHeight; }, 400)`);
  const across = evaluate(`(${FRAMES})(() => { const s = ${SCROLLER}; s.scrollLeft += 200; return s.scrollLeft + s.clientWidth < s.scrollWidth; }, 400)`);
  const diagonal = evaluate(`(${FRAMES})(() => { const s = ${SCROLLER}; s.scrollLeft -= 150; s.scrollTop -= 90; return s.scrollLeft > 0 || s.scrollTop > 0; }, 400)`);
  const gaps = [...down, ...across, ...diagonal];
  const worst = Math.max(...gaps);
  const sorted = [...gaps].sort((a, b) => a - b);
  const timing = {
    network: size,
    frames: gaps.length,
    averageMs: Math.round((gaps.reduce((a, b) => a + b, 0) / gaps.length) * 10) / 10,
    p95Ms: Math.round((sorted[Math.floor(0.95 * sorted.length)] ?? 0) * 10) / 10,
    worstMs: Math.round(worst * 10) / 10,
  };
  check("scrolled the whole network", down.length > 10 && across.length > 10, { down: down.length, across: across.length });
  check("no frame over 50 ms while scrolling", worst <= 50, timing);
  console.log(JSON.stringify(timing));

  // Clicking a box selects its activity in the grid.
  evaluate(`(${SCROLLER}.scrollTo(0, 0), true)`);
  pause(300);
  // A box other than the first, so the selection has to move to it.
  cli("run-code", 'async (page) => { await page.locator(".project-network .network-box[aria-label^=\'A12 Activity 12:\']").click(); }');
  const SELECTED = '[...document.querySelectorAll("#grid-mirror tr")].find((tr) => tr.querySelector("[aria-selected=true]"))?.querySelector("td")?.textContent ?? null';
  const CENTRE = 'document.querySelector(".project-network .network-box.centre")?.getAttribute("aria-label")?.split(" ")[0] ?? null';
  check("a box clicked selects exactly its activity", waitFor(`(${SELECTED}) === "A12" && (${CENTRE}) === "A12"`), [evaluate(SELECTED), evaluate(CENTRE)]);

  // A filter in the grid narrows the network.
  fill("Filter activities by ID or name", "Activity 1");
  check("the grid's filter narrows the network", waitFor(`(${COUNT})?.startsWith("1") && !(${COUNT}).startsWith("2,000")`), evaluate(COUNT));
  fill("Filter activities by ID or name", "");

  if (shot) {
    evaluate(`(${SCROLLER}.scrollTo(0, 0), true)`);
    pause(300);
    cli("screenshot", `--filename=${shot}`);
  }
  cli("press", "Alt+Shift+W");
  check("and its key again goes back to the schedule", waitFor('!document.querySelector(".view-page:not([hidden])") && document.querySelector("[role=tab][aria-selected=true]")?.textContent === "Schedule"'));
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
