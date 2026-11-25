// Reviewing an agent's proposals in the app, in a real browser, through
// playwright-cli, against a store made from a synthetic fixture. A second
// process acts as the agent (`TOTALFLOAT_AUTHOR_KIND=agent`, `edit --propose`):
//   - a proposal appears within 2 s, without a reload;
//   - accepting it updates the grid;
//   - rejecting it needs a reason;
//   - a proposal whose branch moved on is replayed when accepted;
//   - one that no longer applies lists its conflicts and changes nothing.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/review.mjs [--bin target/release/totalfloat] [--screenshot review.png]
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
const session = `review-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-review-e2e-"));
const store = join(dir, "plant.tfdb");

/** Runs totalfloat as the person at the keyboard. */
function totalfloat(...args) {
  return execFileSync(bin, args, { encoding: "utf8", cwd: dir, env: { ...process.env, TOTALFLOAT_AUTHOR: "Pat", TOTALFLOAT_AUTHOR_KIND: "human" } });
}

/** Runs totalfloat as an agent proposing a change. */
function agent(...args) {
  return execFileSync(bin, ["edit", store, ...args, "--propose"], {
    encoding: "utf8",
    cwd: dir,
    env: { ...process.env, TOTALFLOAT_AUTHOR: "planner-bot", TOTALFLOAT_AUTHOR_KIND: "agent" },
  });
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
    pause(100);
  }
  return false;
}

/** A real click on the first element matching `selector` that holds `text`. */
function click(selector, text = "") {
  cli("run-code", `async (page) => { await page.locator(${JSON.stringify(selector)}, { hasText: ${JSON.stringify(text)} }).first().click(); }`);
}

/** Types into the field with this label. */
function fill(label, text) {
  cli("run-code", `async (page) => { await page.getByLabel(${JSON.stringify(label)}, { exact: true }).fill(${JSON.stringify(text)}); }`);
}

/** The changesets on main, newest first, as `history log` lists them. */
function log(...more) {
  return totalfloat("history", "log", store, ...more)
    .split("\n")
    .filter((line) => /^[0-9a-f]{10} /.test(line));
}

const NOTICE = 'document.querySelector(".edit-bar .notice").textContent';
const BADGE = '(document.querySelector(".review-badge .count")?.textContent ?? "0")';
const WAITING = '[...document.querySelectorAll(".review .history-entry .history-message")].map((m) => m.textContent)';

/** An expression for the mirror's text for an activity's cell, by its ID and the column's title. */
function cellIs(code, title) {
  return `(() => {
    const titles = [...document.querySelectorAll("#grid-mirror th")].map((th) => th.textContent);
    const row = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells.length > 1 && tr.cells[0].textContent === ${JSON.stringify(code)});
    const td = row && row.cells[titles.indexOf(${JSON.stringify(title)})];
    return td ? td.textContent : null;
  })()`;
}

/** Proposes as the agent, and checks the page shows it within two seconds, with no reload. */
function proposed(args, reason) {
  const before = evaluate(BADGE);
  const at = Date.now();
  agent(...args, "-m", reason);
  const shown = waitFor(`${BADGE} === "${Number(before) + 1}" && ${WAITING}.includes(${JSON.stringify(reason)})`, 2_000);
  check(`"${reason}" appears within 2 s, with no reload`, shown, { ms: Date.now() - at, waiting: evaluate(WAITING) });
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
  cli("resize", "1500", "900");
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("the page is live", waitFor('document.querySelector(".connection")?.textContent === "Live"'));
  click(".review-badge");
  check("the review panel opens", waitFor('!!document.querySelector(".review")'));

  // A proposal arrives; opened, it shows its impact, lit in the grid; accepted, the grid updates.
  proposed(["--duration", "A2=6d"], "Crane is late");
  click(".review .history-entry", "Crane is late");
  check(
    "opened, it says what it changes",
    waitFor('(document.querySelector(".review .history-applied")?.textContent ?? "").includes("A2 duration")'),
    evaluate('document.querySelector(".review .history-detail")?.textContent ?? null'),
  );
  check(
    "and shows its change beside A2 in the grid",
    waitFor(`(${cellIs("A2", "After")} ?? "").includes("Original duration 6 days")`),
    evaluate(cellIs("A2", "After")),
  );
  const before = log().length;
  click(".review button", "Accept");
  check("accepting it records it on main", waitFor(`${NOTICE}.startsWith("Accepted ")`) && log().length === before + 1, [evaluate(NOTICE), log()[0]]);
  check("and updates the grid", waitFor(`${cellIs("A2", "Original (d)")} === "6"`), evaluate(cellIs("A2", "Original (d)")));
  check("and leaves nothing waiting", waitFor(`${BADGE} === "0"`), evaluate(BADGE));

  // Rejecting needs a reason.
  proposed(["--duration", "A3=9d"], "Pour takes longer");
  click(".review .history-entry", "Pour takes longer");
  click(".review button", "Reject");
  check(
    "Reject waits for a reason",
    waitFor('document.querySelector(".review form button[type=submit]")?.disabled === true'),
  );
  fill("Why reject it", "Pour crew confirmed three days");
  click(".review form button[type=submit]", "Reject");
  check("rejecting it says so", waitFor(`${NOTICE}.startsWith("Rejected ")`), evaluate(NOTICE));
  check("and nothing waits", waitFor(`${BADGE} === "0"`), evaluate(BADGE));
  check("main is unchanged", log().length === before + 1, log()[0]);
  const rejected = log("--all").find((line) => line.includes("rejected"));
  check("the rejected proposal stays in the history", !!rejected, log("--all").slice(0, 3));

  // A proposal whose branch moved on is replayed on its head.
  proposed(["--duration", "A4=4d"], "Commissioning needs four days");
  totalfloat("edit", store, "--name", "A1=Mobilise", "-m", "Rename");
  click(".review .history-entry", "Commissioning needs four days");
  click(".review button", "Accept");
  check("accepting after main moved replays it", waitFor(`${NOTICE}.includes("replayed")`), evaluate(NOTICE));
  check("with both changes in the grid", waitFor(`${cellIs("A4", "Original (d)")} === "4" && ${cellIs("A1", "Activity name")} === "Mobilise"`), [
    evaluate(cellIs("A4", "Original (d)")),
    evaluate(cellIs("A1", "Activity name")),
  ]);

  // One that no longer applies lists why, and changes nothing.
  proposed(["--duration", "A3=5d"], "Rebar takes five days");
  totalfloat("edit", store, "--delete", "A3", "-m", "A3 not needed");
  const moved = log().length;
  click(".review .history-entry", "Rebar takes five days");
  click(".review button", "Accept");
  check(
    "a proposal that no longer applies lists its conflicts",
    waitFor('(document.querySelector(".review-conflicts")?.textContent ?? "").includes("A3")'),
    evaluate('document.querySelector(".review-conflicts")?.textContent ?? null'),
  );
  check("and changes nothing", log().length === moved && evaluate(BADGE) === "1", [log()[0], evaluate(BADGE)]);

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
