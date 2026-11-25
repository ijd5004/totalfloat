// The app's speed on a large schedule, in a real browser, through
// playwright-cli: first paint, scrolling the grid and the Gantt with logic
// lines on, scrolling the grid grouped two levels deep, an edit previewed
// and then recorded, switching branches, and opening a proposal. `cargo xtask ui-bench` builds the 50k schedule, serves
// it, and runs this; it prints one JSON object of timings, in milliseconds,
// as its last line, with `error` set if a step failed.
//
//   node ui/e2e/bench.mjs '<sign-in link from totalfloat serve>'
//
// Each action is started inside the page and timed there, from the moment
// it's started to the moment the page shows its result, so the time
// playwright-cli takes to send a command isn't counted.
import { execFileSync } from "node:child_process";

const [link] = process.argv.slice(2);
if (!link) {
  console.error("usage: node ui/e2e/bench.mjs '<sign-in link from totalfloat serve>'");
  process.exit(2);
}
const origin = new URL(link).origin;
const session = `bench-${process.pid}`;

function cli(...args) {
  return execFileSync("playwright-cli", [`-s=${session}`, ...args], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

/** Runs `expression` in the page, awaiting a promise, and returns its JSON result. */
function evaluate(expression) {
  const out = cli("eval", expression);
  const match = /### Result\n([\s\S]*?)(?:\n### |$)/.exec(out);
  if (!match) throw new Error(`no result from ${expression.slice(0, 200)}:\n${out}`);
  return JSON.parse(match[1].trim());
}

const sleeper = new Int32Array(new SharedArrayBuffer(4));
function pause(ms) {
  Atomics.wait(sleeper, 0, 0, ms);
}

function waitFor(expression, ms = 120_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (evaluate(expression)) return true;
    pause(200);
  }
  throw new Error(`timed out waiting for ${expression}`);
}

/** The page's helpers: frame timing, and waiting for a condition, timed from a start. */
const HELPERS = `(() => {
  window.__bench = {
    /** Runs step() once a frame for n frames, or until it says to stop, and gives the frame gaps. */
    frames(step, n) {
      return new Promise((done) => {
        const gaps = [];
        let last = 0;
        let count = 0;
        const tick = (now) => {
          if (last) gaps.push(now - last);
          last = now;
          if (count++ >= n || step() === false) {
            done(gaps);
            return;
          }
          requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      });
    },
    /** Resolves with the milliseconds from start to when ready() first holds, checked each frame. */
    until(start, ready, limit = 60000) {
      return new Promise((done, fail) => {
        const check = () => {
          if (ready()) done(performance.now() - start);
          else if (performance.now() - start > limit) fail(new Error("timed out"));
          else requestAnimationFrame(check);
        };
        check();
      });
    },
    painted() {
      const marks = performance.getEntriesByName("totalfloat:grid-painted");
      return marks.length ? marks[marks.length - 1].startTime : -1;
    },
    notice() {
      return document.querySelector(".edit-bar .notice")?.textContent ?? "";
    },
  };
  return true;
})()`;

/** Average, 95th percentile and worst of frame gaps. */
function summarize(gaps) {
  const sorted = [...gaps].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
  const average = gaps.reduce((a, b) => a + b, 0) / Math.max(gaps.length, 1);
  return { frames: gaps.length, averageMs: round(average), p95Ms: round(at(0.95)), worstMs: round(sorted.at(-1) ?? 0) };
}

function round(ms) {
  return Math.round(ms * 10) / 10;
}

const out = {};
try {
  cli("open", link);
  cli("resize", "1600", "1000");
  // Signed in now: load the app afresh, and time it from navigation to the grid's first paint.
  cli("goto", `${origin}/`);
  waitFor('performance.getEntriesByName("totalfloat:grid-painted").length > 0');
  out.firstPaintMs = round(evaluate('performance.getEntriesByName("totalfloat:grid-painted")[0].startTime'));
  // The grid's lines: activities and their WBS bands.
  out.rows = evaluate('Number(document.querySelector("#grid-mirror").getAttribute("aria-rowcount")) - 1');
  evaluate(HELPERS);
  // Let the first paint's work settle before timing frames.
  pause(1500);

  // The grid, scrolled top to bottom, two screens a frame, with the Gantt drawn beside it and its logic lines on.
  out.gridScroll = summarize(
    evaluate(`(() => {
      const body = document.querySelector(".grid-body");
      body.scrollTop = 0;
      const step = body.clientHeight * 2;
      return window.__bench.frames(() => {
        if (body.scrollTop + body.clientHeight >= body.scrollHeight - 1) return false;
        body.scrollTop += step;
      }, 5000);
    })()`),
  );
  evaluate('(document.querySelector(".grid-body").scrollTop = 0, true)');
  pause(500);

  // The Gantt, scrolled across in time, half a screen a frame, and back.
  out.ganttScroll = summarize(
    evaluate(`(() => {
      const body = document.querySelector(".gantt-body");
      body.scrollLeft = 0;
      let way = 1;
      const step = body.clientWidth / 2;
      return window.__bench.frames(() => {
        if (body.scrollLeft + body.clientWidth >= body.scrollWidth - 1) way = -1;
        if (body.scrollLeft <= 0) way = 1;
        body.scrollLeft += way * step;
      }, 300);
    })()`),
  );
  evaluate('(document.querySelector(".gantt-body").scrollLeft = 0, true)');
  pause(500);

  // The grid grouped two levels deep, WBS then status, scrolled as before.
  const ROWS = 'Number(document.querySelector("#grid-mirror").getAttribute("aria-rowcount")) - 1';
  cli("run-code", 'async (page) => { await page.getByLabel("Then group by", { exact: true }).selectOption("status"); }');
  waitFor(`(${ROWS}) > ${out.rows}`);
  out.groupedRows = evaluate(ROWS);
  pause(1000);
  out.gridScrollGrouped = summarize(
    evaluate(`(() => {
      const body = document.querySelector(".grid-body");
      body.scrollTop = 0;
      const step = body.clientHeight * 2;
      return window.__bench.frames(() => {
        if (body.scrollTop + body.clientHeight >= body.scrollHeight - 1) return false;
        body.scrollTop += step;
      }, 5000);
    })()`),
  );
  evaluate('(document.querySelector(".grid-body").scrollTop = 0, true)');
  cli("run-code", 'async (page) => { await page.getByLabel("Then by, level 2", { exact: true }).selectOption(""); }');
  waitFor(`(${ROWS}) === ${out.rows}`);
  pause(500);

  // An edit: A1's original duration typed, then Enter: the dry run's preview shown.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  cli("press", "ArrowDown");
  cli("press", "ArrowDown");
  cli("press", "ArrowRight");
  cli("press", "ArrowRight");
  cli("press", "9");
  waitFor('!!document.querySelector(".cell-input")');
  out.editPreviewMs = round(
    evaluate(`(() => {
      const start = performance.now();
      document.querySelector(".cell-input").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      return window.__bench.until(start, () => window.__bench.notice().startsWith("Preview:"));
    })()`),
  );
  // Enter again: recorded, and the grid painted afresh with it.
  out.editRecordedMs = round(
    evaluate(`(() => {
      const start = performance.now();
      document.querySelector(".grid-body").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      return window.__bench.until(start, () => window.__bench.notice().startsWith("Recorded") && window.__bench.painted() > start);
    })()`),
  );
  pause(500);

  // Switching to another branch, until it's shown and painted.
  cli("run-code", 'async (page) => { await page.locator(".branch-button").first().click(); }');
  waitFor('[...document.querySelectorAll(".branch-switch")].some((b) => b.textContent.includes("what-if") && !b.disabled)');
  out.branchSwitchMs = round(
    evaluate(`(() => {
      const start = performance.now();
      [...document.querySelectorAll(".branch-switch")].find((b) => b.textContent.includes("what-if")).click();
      return window.__bench.until(start, () =>
        (document.querySelector(".branch-button .fact-value")?.textContent ?? "").startsWith("what-if") && window.__bench.painted() > start);
    })()`),
  );
  pause(500);

  // Opening a proposal: its comparison against where it was made shown.
  cli("run-code", 'async (page) => { await page.locator(".review-badge").first().click(); }');
  waitFor('!!document.querySelector(".review .history-entry")');
  out.proposalOpenMs = round(
    evaluate(`(() => {
      const start = performance.now();
      document.querySelector(".review .history-entry").click();
      return window.__bench.until(start, () => !!document.querySelector(".review .history-result"));
    })()`),
  );
  out.browser = evaluate("navigator.userAgent");
} catch (error) {
  // The timings taken so far are still printed, with what went wrong.
  out.error = String(error?.message ?? error).split("\n")[0];
  process.exitCode = 1;
} finally {
  try {
    cli("close");
  } catch {
    // Already closed.
  }
}
console.log(JSON.stringify(out));
