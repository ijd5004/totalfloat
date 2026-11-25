// The History panel's graph in a real browser, through playwright-cli, on a
// store made from the synthetic MS Project fixture:
//   - main holds three edits after the import; a what-if branch starts two
//     changes back and holds two edits; it's merged into main;
//   - an agent, through MCP, proposes an edit on main;
//   - the panel draws two lanes joined at the merge, the what-if line starting
//     at the change it was made from, both names at their heads, a baseline
//     label, and the proposal as a hollow dot once proposals are shown;
//   - the PROPOSAL tag's text reads on its background in both themes (4.5:1);
//   - clicking a dot selects the change, as clicking its row does.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/history-graph.mjs [--bin target/release/totalfloat] [--screenshot graph.png]
//
// With --screenshot, saves the panel in the light theme there and in the dark
// theme beside it (graph-dark.png). Exits non-zero if a check fails.
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
const session = `history-graph-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-history-graph-e2e-"));
const store = join(dir, "plant.tfdb");
const env = { ...process.env, TOTALFLOAT_AUTHOR: "Pat" };

function totalfloat(...args) {
  return execFileSync(bin, args, { encoding: "utf8", cwd: dir, env });
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

/**
 * An expression for the WCAG contrast ratio of the first `selector`'s text
 * against its own background, or null if it isn't on the page.
 */
function contrastOf(selector) {
  return `(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const style = getComputedStyle(el);
    const lum = (css) => {
      const [r, g, b] = css.match(/[\\d.]+/g).slice(0, 3).map((v) => {
        const c = Number(v) / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const [hi, lo] = [lum(style.color), lum(style.backgroundColor)].sort((a, b) => b - a);
    return Math.round(((hi + 0.05) / (lo + 0.05)) * 100) / 100;
  })()`;
}

/** Checks `selector`'s text reads (WCAG AA, 4.5:1) in the light and dark themes. */
function checkContrast(name, selector) {
  for (const scheme of ["light", "dark"]) {
    cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: "${scheme}" }); }`);
    pause(150);
    const ratio = evaluate(contrastOf(selector));
    check(`${name} reads in the ${scheme} theme (4.5:1)`, ratio !== null && ratio >= 4.5, ratio);
  }
  cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: null }); }`);
}

/** The newest changeset's ID on a branch. */
function head(branch) {
  return JSON.parse(totalfloat("history", "log", store, "--branch", branch, "--limit", "1", "--json")).changesets[0].id;
}

/** Makes an edit through MCP, as an agent, proposed rather than recorded. */
async function proposeThroughMcp(edits, reason) {
  const mcp = spawn(bin, ["mcp"], { cwd: dir, env: { ...env, TOTALFLOAT_AUTHOR: "planner-bot", TOTALFLOAT_AUTHOR_KIND: "agent" } });
  let buffer = "";
  const waiting = new Map();
  mcp.stdout.on("data", (chunk) => {
    buffer += chunk;
    let at;
    while ((at = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, at);
      buffer = buffer.slice(at + 1);
      if (!line.trim()) continue;
      const message = JSON.parse(line);
      waiting.get(message.id)?.(message);
    }
  });
  let next = 0;
  const send = (method, params) =>
    new Promise((done) => {
      const id = ++next;
      waiting.set(id, done);
      mcp.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  const call = async (name, args) => {
    const answer = await send("tools/call", { name, arguments: args });
    const text = answer.result?.content?.[0]?.text ?? "";
    if (answer.error || answer.result?.isError) throw new Error(`${name} failed: ${JSON.stringify(answer)}`);
    return JSON.parse(text);
  };
  await send("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "history-graph-e2e", version: "1" } });
  mcp.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
  const { handle } = await call("open", { path: store });
  const done = await call("edit", { handle, edits, reason, propose: true });
  mcp.stdin.end();
  mcp.kill();
  return done;
}

const DOTS = `(() => Object.fromEntries([...document.querySelectorAll(".history-entry")].map((entry) => {
  const li = entry.closest("li");
  const dot = li.querySelector(".history-dot");
  return [entry.querySelector(".history-message").firstChild.textContent, {
    cx: Number(dot.getAttribute("cx")),
    hollow: dot.classList.contains("hollow"),
    names: [...li.querySelectorAll(".history-ref.branch")].map((r) => r.textContent),
    baselines: [...li.querySelectorAll(".history-ref.baseline")].map((r) => r.textContent),
    paths: [...li.querySelectorAll(".history-graph path")].map((p) => p.getAttribute("d")),
  }];
})))()`;

let server;
try {
  copyFileSync(join(repo, "fixtures/msproject/plant.xml"), join(dir, "plant.xml"));
  totalfloat("import", "plant.xml", "-o", "plant.xer");
  totalfloat("history", "init", "plant.xer", "-m", "As planned");
  const codes = totalfloat("query", store, "--columns", "code")
    .split("\n")
    .slice(1)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("Showing"));
  check("the fixture has five activities", codes.length === 5, codes);
  const [a, b, c, d, e] = codes;
  totalfloat("edit", store, "--duration", `${a}=6d`, "-m", "Excavation takes six days");
  totalfloat("edit", store, "--duration", `${b}=4d`, "-m", "Slab pour shortened");
  totalfloat("history", "baseline", store, "submitted");
  totalfloat("edit", store, "--duration", `${c}=3d`, "-m", "Fence crew confirmed");
  // A what-if two changes back from main's head, holding two changes.
  const start = JSON.parse(totalfloat("history", "log", store, "--json")).changesets[2].id;
  totalfloat("history", "branch", store, "what-if", "--at", start);
  // Renames, as the last activity is a milestone and has no duration.
  totalfloat("edit", `${store}@what-if`, "--name", `${d}=Yard works (longer)`, "-m", "What if the yard takes longer");
  totalfloat("edit", `${store}@what-if`, "--name", `${e}=Handover`, "-m", "Handover in two days");
  const whatIf = head("what-if");
  totalfloat("history", "merge", store, "what-if", "-m", "Bring the what-if back");
  const proposed = await proposeThroughMcp([{ op: "set_duration", activity: a, original: "7d" }], "Ground is harder than surveyed");
  check("the agent's edit is a proposal", proposed.recorded?.proposal?.startsWith("proposal/main/") === true, proposed.recorded);

  // The log gives the merge its second parent, and nothing else one.
  const logged = JSON.parse(totalfloat("history", "log", store, "--json")).changesets;
  const merge = logged.find((entry) => entry.kind === "merge");
  check("the log gives the merge its second parent", merge?.second_parent === whatIf, merge);
  check(
    "no other change has a second parent",
    logged.filter((entry) => entry.kind !== "merge").every((entry) => entry.second_parent === null),
  );

  server = spawn(bin, ["serve", store], { cwd: dir, env, stdio: ["ignore", "pipe", "inherit"] });
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
  cli("press", "Alt+h");
  check("Alt+H opens the history panel", waitFor('document.querySelectorAll(".history-log .history-dot").length === 7'));

  const rows = evaluate(DOTS);
  const lane = (cx) => (cx - 7) / 12;
  const mainRows = ["Bring the what-if back", "Fence crew confirmed", "Slab pour shortened", "Excavation takes six days", "As planned"];
  const whatIfRows = ["Handover in two days", "What if the yard takes longer"];
  check("main's changes are in the first lane", mainRows.every((m) => lane(rows[m]?.cx) === 0), mainRows.map((m) => rows[m]?.cx));
  check("what-if's changes are in the second", whatIfRows.every((m) => lane(rows[m]?.cx) === 1), whatIfRows.map((m) => rows[m]?.cx));
  check("the merge joins the lanes", rows["Bring the what-if back"]?.paths.length === 1, rows["Bring the what-if back"]);
  check(
    "what-if's line starts at the change it was made from",
    rows["Excavation takes six days"]?.paths.some((p) => p.startsWith("M 19 0")),
    rows["Excavation takes six days"],
  );
  check("main's name is at its head", JSON.stringify(rows["Bring the what-if back"]?.names) === '["main"]');
  check("what-if's name is at its head", JSON.stringify(rows["Handover in two days"]?.names) === '["what-if"]');
  check("the baseline is a label on its change", JSON.stringify(rows["Slab pour shortened"]?.baselines) === '["submitted"]');
  check("the proposal isn't listed until asked for", rows["Ground is harder than surveyed"] === undefined);

  cli("run-code", `async (page) => { await page.getByLabel("Show proposals, waiting and decided").check(); }`);
  check("the proposal is listed", waitFor('document.querySelectorAll(".history-log .history-dot").length === 8'));
  const shown = evaluate(DOTS);
  const proposal = shown["Ground is harder than surveyed"];
  check("the proposal is a hollow dot off its branch", proposal?.hollow === true && lane(proposal.cx) === 2, proposal);
  checkContrast("the PROPOSAL tag", ".history-tag.proposal");

  // A click on a dot chooses its change.
  cli(
    "run-code",
    `async (page) => { await page.locator(".history-entry", { hasText: "Fence crew confirmed" }).locator("xpath=ancestor::li").locator(".history-dot").click({ force: true }); }`,
  );
  check(
    "clicking a dot selects its change",
    waitFor(
      '(() => { const p = [...document.querySelectorAll(".history-entry[aria-pressed=\\"true\\"]")]; return p.length === 1 && p[0].textContent.includes("Fence crew confirmed") && !!document.querySelector(".history-detail"); })()',
    ),
  );
  cli(
    "run-code",
    `async (page) => { await page.locator(".history-entry", { hasText: "Fence crew confirmed" }).locator("xpath=ancestor::li").locator(".history-dot").click({ force: true }); }`,
  );
  check("clicking it again closes it, as the row does", waitFor('!document.querySelector(".history-entry[aria-pressed=\\"true\\"]")'));

  if (shot) {
    // With a change open, so the shot shows the lines running past its details.
    cli("run-code", `async (page) => { await page.locator(".history-entry", { hasText: "What if the yard takes longer" }).click(); }`);
    waitFor('!!document.querySelector(".history-detail .history-buttons")');
    const panel = `async (page) => { await page.locator("#view-history").screenshot({ path: %PATH% }); }`;
    cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: "light" }); }`);
    pause(300);
    cli("run-code", panel.replace("%PATH%", JSON.stringify(shot)));
    cli("run-code", `async (page) => { await page.emulateMedia({ colorScheme: "dark" }); }`);
    pause(300);
    cli("run-code", panel.replace("%PATH%", JSON.stringify(shot.replace(/(\.png)?$/, "-dark.png"))));
    console.log(`saved ${shot} and its dark twin`);
  }
} catch (error) {
  console.error(error);
  failed = true;
} finally {
  try {
    cli("close");
  } catch {
    // Already closed.
  }
  server?.kill();
  rmSync(dir, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
