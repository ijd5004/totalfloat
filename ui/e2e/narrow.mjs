// The title block and toolbar in a narrow window, in a real browser, through
// playwright-cli, against a store made from a synthetic fixture with a long
// project name. At each width, with and without the Driving path chip:
//   - no two of the title block's or the toolbar's controls overlap, and none
//     is cut off by the bar it's in;
//   - the project name gives way with an ellipsis, and its tooltip has it whole;
//   - the data date, finish and float show whole;
//   - every view's tab is on screen and opens its view.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/narrow.mjs [--bin target/release/totalfloat] [--screenshots DIR]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
const session = `narrow-e2e-${process.pid}`;
const dir = mkdtempSync(join(tmpdir(), "totalfloat-narrow-e2e-"));
const store = join(dir, "plant.tfdb");

/** The widths checked: the narrowest the app supports (docs/guide/serve.md), and 1,280. */
const WIDTHS = [1024, 1280];
const NAME = "Synthetic Project One North Plant Expansion Phase Two";

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

/**
 * An expression: the title block's and the toolbar's controls that overlap
 * another, or that the bar they're in (or a part of it that clips) cuts off.
 */
const LAYOUT = `(() => {
  const controls = [];
  const named = (el) => (el.getAttribute("aria-label") || el.textContent.trim() || el.className).replace(/\\s+/g, " ").slice(0, 40);
  const shown = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden"; };
  const title = document.querySelector(".title-block");
  for (const el of title.querySelectorAll(".mark, .project, [role=tab], .branch-button, .fact, .read-only, .search, .review-badge, .connection, .menu-button > button")) {
    if (shown(el)) controls.push({ bar: title, el });
  }
  const toolbar = document.querySelector(".toolbar");
  for (const el of toolbar.querySelectorAll("button, input, select, .chip")) {
    // Not what's inside a chip or a popup, which floats over the work.
    if (el.parentElement.closest(".chip, .tool-panel, .filter-builder")) continue;
    if (shown(el)) controls.push({ bar: toolbar, el });
  }
  const overlaps = [];
  for (let i = 0; i < controls.length; i++) {
    for (let j = i + 1; j < controls.length; j++) {
      const [a, b] = [controls[i].el, controls[j].el];
      if (a.contains(b) || b.contains(a)) continue;
      const [p, q] = [a.getBoundingClientRect(), b.getBoundingClientRect()];
      const w = Math.min(p.right, q.right) - Math.max(p.left, q.left);
      const h = Math.min(p.bottom, q.bottom) - Math.max(p.top, q.top);
      if (w > 0.5 && h > 0.5) overlaps.push(named(a) + " / " + named(b));
    }
  }
  const cut = [];
  for (const { bar, el } of controls) {
    const r = el.getBoundingClientRect();
    for (let up = el.parentElement; up && bar.contains(up); up = up.parentElement) {
      const style = getComputedStyle(up);
      if (up !== bar && style.overflowX === "visible") continue;
      const box = up.getBoundingClientRect();
      if (r.left < box.left - 0.5 || r.right > box.right + 0.5) {
        cut.push(named(el));
        break;
      }
    }
    if (r.right > document.documentElement.clientWidth + 0.5) cut.push(named(el) + " (off screen)");
  }
  return { controls: controls.length, overlaps, cut };
})()`;

/** An expression: the facts (data date, finish, float) whose label or value is cut short. */
const FACTS_CUT = `[...document.querySelectorAll(".title-block .fact-label, .title-block .fact-value")]
  .filter((el) => el.scrollWidth > el.clientWidth + 1).map((el) => el.textContent)`;

const VIEW = '[...document.querySelectorAll(".tb-views [role=tab]")].find((t) => t.getAttribute("aria-selected") === "true")?.textContent';

let server;
try {
  // The usage fixture with a long project name, so the name has to give way.
  const xer = readFileSync(join(repo, "fixtures/xer/usage.xer"), "latin1");
  check("the fixture names its project as expected", xer.includes("Synthetic Project One"));
  writeFileSync(join(dir, "plant.xer"), xer.replaceAll("Synthetic Project One", NAME), "latin1");
  totalfloat("history", "init", "plant.xer", "-m", "Start");

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
  for (const width of WIDTHS) {
    cli("resize", String(width), "768");
    check(`${width}: the grid loads`, waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
    for (const chip of [false, true]) {
      const at = `${width}${chip ? ", Driving path chip" : ""}`;
      if (chip) {
        // Select A2, then Alt+Shift+D: the toolbar gains the Driving path chip.
        evaluate(`(() => {
          const rows = [...document.querySelectorAll("#grid-mirror tbody tr")];
          const line = rows.findIndex((tr) => tr.cells[0]?.textContent === "A2");
          const body = document.querySelector(".gantt-body");
          const box = body.getBoundingClientRect();
          body.focus();
          body.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: box.left + 5, clientY: box.top + (Number(rows[line].getAttribute("aria-rowindex")) - 2) * 22 + 11 }));
          return true;
        })()`);
        cli("press", "Alt+Shift+D");
        check(`${at}: the chip shows`, waitFor('[...document.querySelectorAll(".toolbar button")].some((b) => b.textContent.trim() === "Driving path")'));
      }
      const layout = evaluate(LAYOUT);
      check(`${at}: no controls overlap`, layout.controls > 15 && layout.overlaps.length === 0, layout);
      check(`${at}: none is cut off`, layout.cut.length === 0, layout.cut);
      check(`${at}: the figures show whole`, evaluate(FACTS_CUT).length === 0, evaluate(FACTS_CUT));
      const project = evaluate(`(() => { const b = document.querySelector(".title-block .project"); const n = b.querySelector(".project-name"); return { cut: n.scrollWidth > n.clientWidth, title: b.title }; })()`);
      check(`${at}: the project name gives way, and its tooltip has it whole`, project.title.includes(NAME) && (width > 1500 || project.cut), project);
      if (shots) cli("screenshot", `--filename=${join(shots, `narrow-${width}${chip ? "-driving" : ""}.png`)}`);
    }
    // Every view's tab is where a click lands, and opens its view.
    const tabs = evaluate('[...document.querySelectorAll(".tb-views [role=tab]")].map((t) => t.textContent)');
    for (const tab of tabs) {
      const hit = evaluate(`(() => {
        const t = [...document.querySelectorAll(".tb-views [role=tab]")].find((t) => t.textContent === ${JSON.stringify(tab)});
        const r = t.getBoundingClientRect();
        return document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2) === t;
      })()`);
      cli("run-code", `async (page) => { await page.getByRole("tab", { name: ${JSON.stringify(tab)}, exact: true }).click(); }`);
      check(`${width}: ${tab}'s tab takes the click and opens it`, hit && waitFor(`(${VIEW}) === ${JSON.stringify(tab)}`), evaluate(VIEW));
    }
    cli("run-code", `async (page) => { await page.getByRole("tab", { name: "Schedule", exact: true }).click(); }`);
    // The driving path off again for the next width.
    cli("press", "Alt+Shift+D");
  }
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
