// Seeing and changing a resource's limit in the Usage panel, in a real
// browser, through playwright-cli, against a store made from the
// synthetic fixtures/xer/usage.xer:
//   - the panel says the crew's limit, 1 unit an hour, and draws its line;
//   - 2 units an hour set there waits as an edit with a preview, and Apply
//     records it: the panel says 2, every period's limit is the server's
//     and twice what it was, and the limit line moves up;
//   - 3 from a day on waits and records the same way, so the limit says
//     when it changes;
//   - Clear limit records no limit: the panel says so and the line goes.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/usage-limit.mjs [--bin target/release/totalfloat] [--screenshot limit]
//
// --screenshot PREFIX writes PREFIX-set-light.png, PREFIX-set-dark.png and
// PREFIX-cleared-light.png. Exits non-zero if a check fails.
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
const session = `usage-limit-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-usage-limit-e2e-"));
const store = join(dir, "plan.tfdb");

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

const NOTICE = 'document.querySelector(".edit-bar .notice")?.textContent ?? ""';
const PENDING = 'document.querySelector(".pending")?.textContent ?? ""';
const SAID = 'document.querySelector(".usage-limit-now")?.textContent ?? ""';
/** The limit line's path, and each table row's limit cell. */
const DRAWN = `(() => ({
  path: document.querySelector(".usage-histogram .usage-limit")?.getAttribute("d") ?? "",
  limits: [...document.querySelectorAll(".usage-table tbody tr")].map((tr) => tr.children[3]?.textContent ?? ""),
}))()`;

/** The server's answer, asked from the page with its session. */
const ASK = `(async () => {
  const session = localStorage.getItem("totalfloat-session");
  const answer = await fetch("/api/1/usage?resource=CREW&by=week", { headers: { "x-totalfloat-session": session ?? "" } });
  return answer.json();
})()`;

/** Units as the page shows them. */
const units = (n) => (Math.round(n * 100) / 100).toLocaleString("en-US", { maximumFractionDigits: 2 });

/** The limit line's highest point: the smallest y it reaches, or null without one. */
function top(path) {
  const ys = [...path.matchAll(/[MV]\s*(?:[\d.]+\s+)?([\d.]+)/g)].map((m) => Number(m[1]));
  return ys.length ? Math.min(...ys) : null;
}

function fill(label, value) {
  cli("run-code", `async (page) => { await page.getByLabel(${JSON.stringify(label)}, { exact: true }).fill(${JSON.stringify(value)}); }`);
}

function click(name) {
  cli("run-code", `async (page) => { await page.getByRole("button", { name: ${JSON.stringify(name)}, exact: true }).click(); }`);
}

function apply() {
  cli("run-code", 'async (page) => { await page.getByRole("group", { name: "Edits pending" }).getByRole("button", { name: "Apply" }).click(); }');
}

/** Each period's limit cell matches what the server answers now. */
function matchesServer(what) {
  const answer = evaluate(ASK);
  const drawn = evaluate(DRAWN);
  const same = answer.periods.every((p, k) => {
    const cell = drawn.limits[k] ?? "";
    return p.limit === null ? cell.startsWith("none") : cell.startsWith(units(p.limit));
  });
  check(`${what}: each period's limit is the server's`, same && drawn.limits.length === answer.periods.length, {
    periods: answer.periods.length,
  });
  return answer;
}

let server;
try {
  copyFileSync(join(repo, "fixtures/xer/usage.xer"), join(dir, "plan.xer"));
  totalfloat("history", "init", "plan.xer", "-m", "Start");
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
  cli("resize", "1500", "900");
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+U");
  check("the usage panel opens", waitFor('!!document.querySelector("section.usage")'));
  fill("Resource", "CREW");
  click("Show");
  check("it says the limit in force", waitFor(`(${SAID}) === "Limit 1 unit an hour."`), evaluate(SAID));
  const first = matchesServer("at 1 an hour");
  const before = top(evaluate(DRAWN).path);
  check("the limit line is drawn", before !== null, evaluate(DRAWN).path);

  // 2 an hour, throughout.
  fill("Limit, units an hour", "2");
  click("Set limit");
  check("it waits as an edit, previewed", waitFor(`(${PENDING}).startsWith("1 edit")`), evaluate(PENDING));
  apply();
  check("Apply records it", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  check("the panel says 2", waitFor(`(${SAID}) === "Limit 2 units an hour."`), evaluate(SAID));
  const doubled = matchesServer("at 2 an hour");
  check(
    "every period's limit doubled",
    doubled.periods.every((p, k) => p.limit === first.periods[k].limit * 2),
    doubled.periods.map((p) => p.limit),
  );
  const after = top(evaluate(DRAWN).path);
  check("the limit line moved up", after !== null && before !== null && after < before, { before, after });
  screenshot("set", "light");
  screenshot("set", "dark");

  // 3 from the second week on.
  const second = doubled.periods[1]?.start;
  fill("Limit, units an hour", "3");
  fill("From", second);
  click("Set limit");
  check("a dated limit waits too", waitFor(`(${PENDING}).startsWith("1 edit")`), evaluate(PENDING));
  apply();
  check("Apply records the dated limit", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  check("the panel says when it changes", waitFor(`(${SAID}).startsWith("Limit 2 units an hour; 3 from ")`), evaluate(SAID));
  matchesServer("from a day");

  // Cleared.
  fill("From", "");
  click("Clear limit");
  check("clearing waits as an edit", waitFor(`(${PENDING}).startsWith("1 edit")`), evaluate(PENDING));
  apply();
  check("Apply records it", waitFor(`(${NOTICE}).startsWith("Recorded ")`), evaluate(NOTICE));
  check("the panel says there's no limit", waitFor(`(${SAID}) === "No limit."`), evaluate(SAID));
  const cleared = matchesServer("cleared");
  check("no period has a limit", cleared.periods.every((p) => p.limit === null));
  check("the limit line is gone", top(evaluate(DRAWN).path) === null, evaluate(DRAWN).path);
  screenshot("cleared", "light");

  // The store holds each as a changeset.
  const log = totalfloat("history", "log", store).trim().split("\n");
  check("three changesets after the start", log.length >= 4, log.length);
} catch (error) {
  failed = true;
  console.error(String(error?.message ?? error).split("\n")[0]);
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
