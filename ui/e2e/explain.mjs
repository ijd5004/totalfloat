// The inspector's "Why" section, and its logic, in a real browser, through
// playwright-cli, against a store made from a synthetic fixture: an activity
// driven by an SS lag lists its predecessor with the link's type, lag and
// driving mark, matching `GET /api/1/links`, shows that predecessor's step in
// the chain, and clicking either selects the predecessor.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/explain.mjs [--bin target/release/totalfloat] [--screenshot explain.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
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
const shot = flag("--screenshot") ? resolve(flag("--screenshot")) : null;
const session = `explain-e2e-${process.pid}`;
const dir = mkdtempSync(join(tmpdir(), "totalfloat-explain-e2e-"));
const store = join(dir, "plant.tfdb");

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

/** The active row's activity ID. */
const activeCode = () =>
  evaluate(`(() => {
    const id = document.querySelector(".grid-body").getAttribute("aria-activedescendant");
    return document.getElementById(id)?.closest("tr")?.cells[0]?.textContent ?? null;
  })()`);

/** Clicks an activity's bar row in the Gantt, which selects it and focuses the chart. */
function select(code) {
  const line = evaluate(
    `(() => { const tr = [...document.querySelectorAll("#grid-mirror tbody tr")].find((tr) => tr.cells[0].textContent === ${JSON.stringify(code)}); return Number(tr.getAttribute("aria-rowindex")) - 2; })()`,
  );
  evaluate(`(() => {
    const body = document.querySelector(".gantt-body");
    const box = body.getBoundingClientRect();
    const y = box.top + ${line} * 22 - document.querySelector(".grid-body").scrollTop + 11;
    body.focus();
    body.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: box.left + 5, clientY: y }));
    return true;
  })()`);
}

/**
 * An expression: whether the inspector lists the selected activity's links
 * as `GET /api/1/links` gives them, by other end, type, lag and driving.
 */
const LINKS_MATCH = `(async () => {
  const get = (path) => fetch(path, { headers: { "x-totalfloat-session": localStorage.getItem("totalfloat-session") ?? "" } }).then((r) => r.json());
  const [links, rows] = await Promise.all([get("/api/1/links"), get("/api/1/rows")]);
  const code = document.querySelector(".inspector-code").textContent;
  const id = rows.id[rows.code.indexOf(code)];
  const hours = (k) => links.lag_hours[k];
  const want = (end, other) =>
    links.id
      .map((_, k) => k)
      .filter((k) => links[end][k] === id)
      .map((k) => [rows.code[rows.id.indexOf(links[other][k])], links.type[k], hours(k) === 0, links.driving[k]].join(" "))
      .sort();
  const shown = (title) =>
    [...([...document.querySelectorAll(".inspector-section")].find((s) => s.querySelector("h3")?.textContent.startsWith(title))?.querySelectorAll(".inspector-link-row") ?? [])]
      .map((row) => {
        // Where the store can be edited, the type and lag are fields; else text.
        const text = row.querySelector(".inspector-link-type")?.textContent ?? "";
        const type = row.querySelector(".inspector-link-edit select")?.value ?? text.split(" ")[0];
        const lag = row.querySelector(".inspector-lag");
        const none = lag ? lag.value === "0" : !text.includes(" ");
        return [row.querySelector(".inspector-link-code").textContent, type, none, row.querySelector(".inspector-dot").classList.contains("driving")].join(" ");
      })
      .sort();
  return JSON.stringify(want("successor", "predecessor")) === JSON.stringify(shown("Predecessors")) && JSON.stringify(want("predecessor", "successor")) === JSON.stringify(shown("Successors"));
})()`;

let server;
try {
  // A3 starts a day after A2 starts: an SS lag drives it.
  copyFileSync(join(repo, "fixtures/xer/batches/001/c-start-after.xer"), join(dir, "plant.xer"));
  totalfloat("history", "init", "plant.xer", "-m", "Start");
  const wbs = totalfloat("query", store, "--columns", "code,wbs").split("\n")[1].split(/\s+/)[1];
  totalfloat("edit", store, "--add", `A3;Synthetic activity 3;${wbs};3d`, "--link", "A2>A3:SS+1d", "-m", "SS lag");
  const explained = JSON.parse(totalfloat("explain", store, "A3", "--json"));
  const first = explained.chains[0].steps[0];
  check("the engine says an SS lag from A2 drives A3", /SS/.test(first.text) && explained.chains[0].steps.some((s) => s.code === "A2"), first.text);

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
  cli("resize", "1500", "800");
  check("the grid and Gantt load", waitFor('(document.querySelector(".gantt-canvas")?.width ?? 0) > 300'));

  select("A3");
  check("A3 is selected", waitFor('(() => { const id = document.querySelector(".grid-body").getAttribute("aria-activedescendant"); return document.getElementById(id)?.closest("tr")?.cells[0]?.textContent === "A3"; })()'), activeCode());
  // From the grid, where typing a plain E would edit the cell.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+x");
  check("Alt+X opens the inspector on A3, from the grid", waitFor('document.querySelector(".inspector-code")?.textContent === "A3"'));
  check("its Why section is in view", waitFor('(() => { const why = document.getElementById("inspector-why"); const column = document.querySelector(".right-column"); if (!why || !column) return false; const a = why.getBoundingClientRect(), b = column.getBoundingClientRect(); return a.top >= b.top - 1 && a.top < b.bottom; })()'));
  // The predecessor, as the server's links give it: A2, SS with a day's lag, driving.
  const listed = evaluate('[...document.querySelectorAll(".inspector-section")].find((s) => s.querySelector("h3")?.textContent.startsWith("Predecessors"))?.querySelector(".inspector-link")?.getAttribute("aria-label") ?? null');
  check("the inspector lists A2 as A3's predecessor, SS +1d, driving", listed === "A2 Synthetic activity 2, SS +1d, driving", listed);
  check(
    "the inspector's predecessors and successors match the server's links",
    evaluate(LINKS_MATCH),
    evaluate('[...document.querySelectorAll(".inspector-link")].map((b) => b.getAttribute("aria-label"))'),
  );
  // In plain words: A3 can't start until some time after A2 starts.
  check(
    "it says, plainly, that A3 waits for A2 to start",
    waitFor('[...document.querySelectorAll(".explain-why")].some((w) => /can\'t start until .* after A2 .*starts/.test(w.textContent))'),
    evaluate('[...document.querySelectorAll(".explain-stop")].map((b) => b.textContent)'),
  );
  check(
    "and numbers A3 first and A2 second, as the Gantt does",
    evaluate('[...document.querySelectorAll(".explain-stop")].map((b) => b.querySelector(".explain-n").textContent + " " + b.querySelector("b").textContent).join(", ")') === "1 A3, 2 A2",
    evaluate('[...document.querySelectorAll(".explain-stop")].map((b) => b.querySelector(".explain-n").textContent + " " + b.querySelector("b").textContent)'),
  );
  if (shot) cli("screenshot", `--filename=${shot}`);
  // A2's stop is the predecessor's own.
  evaluate('([...document.querySelectorAll(".explain-stop")].find((b) => b.querySelector("b").textContent === "A2").click(), true)');
  check("clicking A2's step selects A2", waitFor('(() => { const id = document.querySelector(".grid-body").getAttribute("aria-activedescendant"); return document.getElementById(id)?.closest("tr")?.cells[0]?.textContent === "A2"; })()'), activeCode());
  check("and the inspector follows, to A2", waitFor('document.querySelector(".inspector-code")?.textContent === "A2"'));
  // Clicking a successor in the inspector selects it.
  evaluate('([...document.querySelectorAll(".inspector-section")].find((s) => s.querySelector("h3")?.textContent.startsWith("Successors"))?.querySelector(".inspector-link")?.click(), true)');
  check("clicking A3 in A2's successors selects A3", waitFor('document.querySelector(".inspector-code")?.textContent === "A3"'), activeCode());
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
