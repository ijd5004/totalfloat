// The resource usage view in a real browser, through playwright-cli:
// open the Usage panel on a schedule, pick a resource, show its usage by
// week and by month, and check each bar and table row against what
// `/api/1/usage` answers. Give a corpus schedule with --file; it defaults to
// a synthetic fixture. Only counts are printed.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/usage.mjs [--file plan.xer] [--bin target/release/totalfloat] [--screenshot usage.png]
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
const fileAt = flags.indexOf("--file");
const source = resolve(fileAt >= 0 ? flags[fileAt + 1] : join(repo, "fixtures/xer/usage.xer"));
const session = `usage-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-usage-e2e-"));

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


// What the page drew: each bar's period, actual and remaining from its title,
// whether it's marked over, and the table's rows.
const DRAWN = `(() => {
  const bars = [...document.querySelectorAll(".usage-histogram .usage-period")].map((g) => ({
    title: g.querySelector("title")?.textContent ?? "",
    over: g.classList.contains("usage-over"),
  }));
  const rows = [...document.querySelectorAll(".usage-table tbody tr")].map((tr) =>
    [...tr.children].map((c) => c.textContent),
  );
  return { bars, rows };
})()`;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
/** A date as the page shows it by default: 05 Jan 26. */
const shown = (date) => `${date.slice(8, 10)} ${MONTHS[Number(date.slice(5, 7)) - 1]} ${date.slice(2, 4)}`;

/** The server's answer for the same resource and period, asked from the page with its session. */
const ASK = (resource, by) => `(async () => {
  const session = localStorage.getItem("totalfloat-session");
  const url = "/api/1/usage?resource=" + encodeURIComponent(${JSON.stringify(resource)}) + "&by=${by}";
  const answer = await fetch(url, { headers: { "x-totalfloat-session": session ?? "" } });
  return answer.json();
})()`;

/** Units as the page shows them. */
const units = (n) => (Math.round(n * 100) / 100).toLocaleString("en-US", { maximumFractionDigits: 2 });

let server;
try {
  copyFileSync(source, join(dir, "plan.xer"));
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
  check("the grid loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+U");
  check("the usage panel opens", waitFor('!!document.querySelector("section.usage")'));
  const resources = evaluate('[...document.querySelectorAll("#usage-resources option")].map((o) => o.value)');
  check("it lists the file's resources", resources.length > 0, { resources: resources.length });

  // The resource with the most periods, among the first few listed.
  let chosen = null;
  let most = -1;
  for (const resource of resources.slice(0, 8)) {
    const answer = evaluate(ASK(resource, "week"));
    const periods = answer.periods?.length ?? -1;
    if (periods > most) [chosen, most] = [resource, periods];
  }
  check("a resource with units to show", most > 0, { periods: most });

  for (const by of ["week", "month"]) {
    cli("run-code", `async (page) => { await page.getByLabel("Resource", { exact: true }).fill(${JSON.stringify(chosen)}); }`);
    cli("run-code", `async (page) => { await page.getByLabel("By", { exact: true }).selectOption(${JSON.stringify(by)}); }`);
    cli("run-code", 'async (page) => { await page.getByRole("button", { name: "Show" }).click(); }');
    const answer = evaluate(ASK(chosen, by));
    check(`by ${by}: the bars are drawn`, waitFor(
      `((d) => d.bars.length === ${answer.periods.length} && (d.bars[0]?.title ?? "").startsWith(${JSON.stringify(shown(answer.periods[0]?.start ?? ""))}))(${DRAWN})`,
    ), {
      drawn: evaluate(DRAWN).bars.length,
      periods: answer.periods.length,
    });
    const drawn = evaluate(DRAWN);
    const titlesMatch = answer.periods.every((p, k) =>
      (drawn.bars[k]?.title ?? "").startsWith(`${shown(p.start)}: ${units(p.actual)} actual, ${units(p.remaining)} remaining`),
    );
    check(`by ${by}: each bar's actual and remaining are the server's`, titlesMatch);
    const overMatch = answer.periods.every((p, k) => drawn.bars[k]?.over === p.over);
    check(`by ${by}: the periods over the limit are the server's`, overMatch, {
      over: answer.periods.filter((p) => p.over).length,
    });
    const rowsMatch = answer.periods.every((p, k) => {
      const row = drawn.rows[k] ?? [];
      return row[0] === shown(p.start) && row[1] === units(p.actual) && row[2] === units(p.remaining);
    });
    check(`by ${by}: the table's rows are the server's`, rowsMatch, { rows: drawn.rows.length });
  }

  if (shot) cli("screenshot", `--filename=${shot}`);
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
