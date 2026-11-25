// The activity grid in a real browser, through playwright-cli: keyboard
// navigation and copy, and with --timing, first paint and scrolling speed,
// with the Gantt drawn beside it.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/grid.mjs [--bin target/release/totalfloat] [--timing]
//   node ui/e2e/grid.mjs '<sign-in link from totalfloat serve>' [--timing]
//
// Without a link it builds its own schedule from a synthetic fixture: 62
// activities in WBS bands, more lines than a page down covers, over months,
// so the Gantt opens farther out than its nearest scale. Given a link, serve
// a schedule like that: a WBS (the copy check expects bands at the top),
// more than a screen of lines, and more than a few weeks long. For timing,
// serve the 50k schedule `cargo xtask ui-bench` writes.
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const args = process.argv.slice(2);
const timing = args.includes("--timing");
const binAt = args.indexOf("--bin");
const bin = resolve(binAt >= 0 ? args[binAt + 1] : join(repo, "target/release/totalfloat"));
const given = args.find((arg, k) => /^https?:/.test(arg) && args[k - 1] !== "--bin");
const session = `grid-e2e-${process.pid}`;

/**
 * Builds the schedule and serves it: the fixture's two activities, then 60
 * more of 10 to 50 days in two WBS nodes, in six chains of ten.
 */
async function built(dir) {
  const totalfloat = (...a) => execFileSync(bin, a, { encoding: "utf8", cwd: dir });
  copyFileSync(join(repo, "fixtures/xer/batches/001/c-start-after.xer"), join(dir, "small.xer"));
  const root = totalfloat("query", "small.xer", "--columns", "code,wbs").split("\n")[1].split(/\s+/)[1];
  const edits = ["--add-wbs", `W1;Synthetic phase 1;${root}`, "--add-wbs", `W2;Synthetic phase 2;${root}`];
  for (let k = 0; k < 60; k++) {
    edits.push("--add", `B${k + 1};Synthetic activity B${k + 1};W${k < 30 ? 1 : 2};${10 + ((k * 7) % 41)}d`);
    if (k % 10 !== 0) edits.push("--link", `B${k}>B${k + 1}`);
  }
  totalfloat("edit", "small.xer", ...edits, "-o", "grid.xer");
  const server = spawn(bin, ["serve", join(dir, "grid.xer")], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
  const link = await new Promise((done, fail) => {
    let out = "";
    server.stdout.on("data", (chunk) => {
      out += chunk;
      const found = /http:\/\/\S+/.exec(out);
      if (found) done(found[0]);
    });
    server.on("exit", () => fail(new Error(`serve stopped: ${out}`)));
  });
  return { server, link };
}

const dir = given ? null : mkdtempSync(join(tmpdir(), "totalfloat-grid-e2e-"));
const { server, link } = given ? { server: null, link: given } : await built(dir);
const origin = new URL(link).origin;

function cli(...args) {
  return execFileSync("playwright-cli", [`-s=${session}`, ...args], { encoding: "utf8" });
}

/** Runs `expression` in the page and returns its JSON result. */
function evaluate(expression) {
  const out = cli("eval", expression);
  // The result runs to the next section, over several lines for an object.
  const match = /### Result\n([\s\S]*?)(?:\n### |$)/.exec(out);
  if (!match) throw new Error(`no result from ${expression}:\n${out}`);
  return JSON.parse(match[1].trim());
}

let failed = false;
function check(name, ok, detail) {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail === undefined ? "" : `: ${JSON.stringify(detail)}`}`);
  if (!ok) failed = true;
}

/** The active cell, as the grid tells assistive technology: [line, column]. */
function active() {
  const id = evaluate('document.querySelector(".grid-body").getAttribute("aria-activedescendant")');
  return id.replace("grid-cell-", "").split("-").map(Number);
}

function waitFor(expression, ms = 30_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (evaluate(expression)) return true;
    execFileSync(process.execPath, ["-e", "setTimeout(() => {}, 200)"]);
  }
  return false;
}

try {
  cli("open", link);
  cli("resize", "1400", "900");
  // Sign in, then reload with frame timing on; the token is kept.
  cli("goto", `${origin}/?debug=frames`);
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr")'));
  const lines = evaluate('Number(document.querySelector("#grid-mirror").getAttribute("aria-rowcount")) - 1');
  const columns = evaluate('Number(document.querySelector("#grid-mirror").getAttribute("aria-colcount"))');
  check("rows are listed", lines > 0, lines);

  // The Gantt draws beside the grid, so the timing below includes it.
  check(
    "the Gantt is drawn beside the grid",
    waitFor('(document.querySelector(".gantt-canvas")?.width ?? 0) > 300'),
    evaluate('document.querySelector(".gantt-canvas")?.width ?? 0'),
  );

  if (timing) {
    const paint = evaluate(
      '(performance.getEntriesByName("totalfloat:grid-first-paint")[0] || {duration: -1}).duration',
    );
    check("first paint under 2 s after the rows arrive", paint >= 0 && paint < 2000, `${Math.round(paint)} ms`);
    // Scroll the whole list, a step a frame, as a fast drag or wheel would.
    const frames = evaluate(`new Promise((done) => {
      const body = document.querySelector(".grid-body");
      window.__gridFrames.reset();
      body.scrollTop = 0;
      const step = body.clientHeight * 2;
      const tick = () => {
        if (body.scrollTop + body.clientHeight >= body.scrollHeight - 1) {
          requestAnimationFrame(() =>
            requestAnimationFrame(() => {
              window.__gridFrames.stop();
              done({ ...window.__gridFrames });
            }),
          );
          return;
        }
        body.scrollTop += step;
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    })`);
    check(
      "scrolling the whole list averages under 20 ms a frame",
      frames.frames > 0 && frames.averageMs < 20,
      { frames: frames.frames, averageMs: Math.round(frames.averageMs * 10) / 10, worstMs: Math.round(frames.worstMs) },
    );
    evaluate('(document.querySelector(".grid-body").scrollTop = 0, true)');

    // Stepping the selection a row a frame, as a held arrow key does, first
    // with the grid and Gantt alone, then with the inspector open: it
    // follows within the grid's own budget, and ends on the row selected.
    const step = () => {
      evaluate('(document.querySelector(".grid-body").focus(), true)');
      cli("press", "Control+Home");
      return evaluate(`new Promise((done) => {
        const body = document.querySelector(".grid-body");
        window.__gridFrames.reset();
        let left = 150;
        const tick = () => {
          if (left-- === 0) {
            requestAnimationFrame(() =>
              requestAnimationFrame(() => {
                window.__gridFrames.stop();
                const id = body.getAttribute("aria-activedescendant");
                const row = document.getElementById(id)?.closest("tr");
                // A band's line selects no activity.
                const active = !row ? null : row.cells.length > 1 ? row.cells[0].textContent : "No activity selected";
                done({ ...window.__gridFrames, active, shown: document.querySelector(".inspector-code")?.textContent ?? null });
              }),
            );
            return;
          }
          // As a held key repeats: a task between frames, so each draw lands.
          setTimeout(() => {
            body.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
            requestAnimationFrame(tick);
          }, 0);
        };
        requestAnimationFrame(tick);
      })`);
    };
    const rounded = (stats) => ({ frames: stats.frames, averageMs: Math.round(stats.averageMs * 10) / 10, worstMs: Math.round(stats.worstMs) });
    const alone = step();
    check("stepping the selection averages under 20 ms a frame", alone.frames > 0 && alone.averageMs < 20, rounded(alone));
    evaluate('(document.querySelector(".grid-body").focus(), true)');
    cli("press", "Alt+i");
    check("Alt+I opens the inspector", waitFor('!!document.querySelector(".right-column .inspector")'));
    const stepped = step();
    check(
      "and with the inspector open, within the same budget",
      stepped.frames > 0 && stepped.averageMs < 20,
      { alone: rounded(alone), inspector: rounded(stepped) },
    );
    check("and the inspector shows the row selected", stepped.active !== null && stepped.active === stepped.shown, { active: stepped.active, shown: stepped.shown });
    cli("press", "Control+Backslash");
    check("the right column closes again", waitFor('!document.querySelector(".right-column")'));
  }

  // Column titles fit their default widths, with no ellipsis.
  const cut = evaluate('[...document.querySelectorAll(".grid-head-title")].filter((t) => t.scrollWidth > t.clientWidth).map((t) => t.textContent)');
  check("no column title is cut off", cut.length === 0, cut);

  // Alt+= and Alt+- zoom the Gantt's timescale, from the chart and from the
  // grid alike.
  const width = () => evaluate('parseFloat(document.querySelector(".gantt-extent").style.width)');
  // It opens fitted; a step in or out goes to a fixed scale. On a long schedule the fit
  // can be farther out than the farthest scale, so a step out from there stays put.
  evaluate('(document.querySelector(".gantt-body").focus(), true)');
  const fitted = evaluate('document.querySelector(".gantt-body").dataset.zoom') === "fit";
  const wide = width();
  cli("press", "Alt+Equal");
  const nearer = width();
  cli("press", "Alt+Minus");
  const back = width();
  check("Alt+= and Alt+- zoom the Gantt from the chart", fitted && nearer > wide && back <= nearer, { fitted, wide, nearer, back });
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Equal");
  const nearerFromGrid = width();
  cli("press", "Alt+Minus");
  check("and from the grid", nearerFromGrid > back && width() === back, { back, nearerFromGrid });

  // Keyboard: focus the grid at the top left.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  check("Ctrl+Home goes to the first cell", active().join() === "0,0", active());
  cli("press", "ArrowDown");
  cli("press", "ArrowDown");
  check("arrows move down", active()[0] === 2, active());
  cli("press", "End");
  check("End goes to the last column", active()[1] === columns - 1, active());
  cli("press", "Home");
  check("Home goes to the first column", active()[1] === 0, active());
  cli("press", "PageDown");
  check("Page Down moves a screen", active()[0] > 10, active());
  cli("press", "PageUp");
  check("Page Up moves back", active()[0] === 2, active());
  cli("press", "Control+End");
  // A band's line is one cell, as the last is when a store's bands end with Add activity lines.
  const lastIsBand = evaluate(`(document.getElementById("grid-cell-${lines - 1}-0")?.colSpan ?? 1) > 1`);
  check("Ctrl+End goes to the last cell", active()[0] === lines - 1 && active()[1] === (lastIsBand ? 0 : columns - 1), active());
  cli("press", "Control+Home");

  // A range: from the first activity, two rows down and one column right.
  cli("press", "ArrowDown");
  cli("press", "ArrowDown");
  cli("press", "Shift+ArrowDown");
  cli("press", "Shift+ArrowDown");
  cli("press", "Shift+ArrowRight");
  // What the grid put on the clipboard, read from the copy event, so no
  // browser has to grant reading the clipboard (Firefox doesn't).
  evaluate('(window.__copied = null, window.addEventListener("copy", (event) => { if (event.defaultPrevented) window.__copied = event.clipboardData.getData("text/plain"); }, { once: true }), true)');
  cli("press", "Control+c");
  const copied = evaluate("window.__copied") ?? "";
  const rows = copied.split("\n");
  check("Ctrl+C copies the range as tab-separated text", rows.length === 3 && rows.every((r) => r.split("\t").length === 2), copied);

  // The selection follows its activity when the order changes.
  cli("press", "Control+Home");
  for (let k = 0; k < 5; k++) cli("press", "ArrowDown");
  const code = () =>
    evaluate(
      'document.getElementById(document.querySelector(".grid-body").getAttribute("aria-activedescendant")).closest("tr").cells[0].textContent',
    );
  const chosen = code();
  evaluate('(document.querySelectorAll(".grid-head-button")[0].click(), document.querySelectorAll(".grid-head-button")[0].click(), true)');
  check("the selection follows its activity through a sort", code() === chosen, { before: chosen, after: code() });
  evaluate('(document.querySelectorAll(".grid-head-button")[0].click(), document.querySelector(".grid-body").focus(), true)');

  // Columns from the keyboard: Enter sorts, Alt+Shift+Right moves.
  evaluate('(document.querySelectorAll(".grid-head-button")[1].focus(), true)');
  cli("press", "Enter");
  const sortedBy = evaluate('document.querySelector("#grid-mirror th[aria-sort]")?.textContent ?? null');
  cli("press", "Alt+Shift+ArrowRight");
  const heads = evaluate('[...document.querySelectorAll(".grid-head-title")].map((t) => t.textContent).slice(0, 3)');
  check("columns sort and move from the keyboard", sortedBy === "Activity name" && heads[2] === "Activity name", { sortedBy, heads });
  evaluate('(document.querySelectorAll(".grid-head-button")[2].click(), document.querySelectorAll(".grid-head-button")[2].click(), true)');
  cli("press", "Alt+Shift+ArrowLeft");
  evaluate('(document.querySelector(".grid-body").focus(), true)');

  // Two status filters together widen, not narrow.
  const count = () => evaluate('document.querySelector(".grid-count").textContent');
  const all = count();
  cli("run-code", 'async (page) => { for (const name of ["Not started", "In progress"]) { if (!(await page.getByRole("menu").count())) await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitemcheckbox", { name }).click(); } await page.keyboard.press("Escape"); }');
  const open = count();
  cli("run-code", 'async (page) => { for (const name of ["Not started", "In progress"]) { if (!(await page.getByRole("menu").count())) await page.getByRole("button", { name: "Add filter" }).click(); await page.getByRole("menuitemcheckbox", { name }).click(); } await page.keyboard.press("Escape"); }');
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  check("Not started with In progress shows open work", !open.startsWith("0 of"), { all, open });

  // A band collapses with the left arrow and opens with the right.
  cli("press", "Control+Home");
  const before = evaluate('Number(document.querySelector("#grid-mirror").getAttribute("aria-rowcount"))');
  cli("press", "ArrowLeft");
  const collapsed = evaluate('Number(document.querySelector("#grid-mirror").getAttribute("aria-rowcount"))');
  cli("press", "ArrowRight");
  const reopened = evaluate('Number(document.querySelector("#grid-mirror").getAttribute("aria-rowcount"))');
  check("a band collapses and opens from the keyboard", collapsed < before && reopened === before, { before, collapsed, reopened });

  if (timing) {
    // Kept out of the checkout, so it's never committed.
    const shot = join(tmpdir(), "grid-e2e.png");
    cli("screenshot", `--filename=${shot}`);
    console.log(`screenshot: ${shot}`);
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
  if (dir && !failed) rmSync(dir, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
