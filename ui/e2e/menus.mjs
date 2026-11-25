// The Menu button and the views in a real browser, through playwright-cli:
// the Menu opens by mouse and by keyboard, one action from each section runs,
// and each view opens from the title block and from its key, the schedule
// coming back with its scroll and selection. Every key the ? help lists is in
// docs/guide/serve.md's key table.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/menus.mjs [--bin target/release/totalfloat] [--screenshot menus.png]
//
// Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
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
const session = `menus-e2e-${process.pid}`;
const dir = mkdtempSync(join(tmpdir(), "totalfloat-menus-e2e-"));
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

const open = () => evaluate('!!document.querySelector(".menu-list")');
const focused = () => evaluate('document.activeElement?.textContent ?? null');
/** A real pointer click, through Playwright, on what `selector` and `text` find. */
function click(selector, text) {
  cli("run-code", `async (page) => { await page.locator(${JSON.stringify(selector)}, { hasText: ${JSON.stringify(text)} }).first().click(); }`);
}

/** Opens the Menu by mouse, then clicks an item in `section` whose label starts with `item`. */
function choose(section, item) {
  click(".menu-button button", "Menu");
  check(`the Menu opens by mouse, for ${section} > ${item}`, open());
  cli(
    "run-code",
    `async (page) => { await page.getByRole("group", { name: ${JSON.stringify(section)} }).getByRole("menuitem", { name: new RegExp("^" + ${JSON.stringify(item)}) }).click(); }`,
  );
}

const NOTICE = 'document.querySelector(".edit-bar .notice")?.textContent ?? ""';
const RIGHT = 'document.querySelector(".right-column")?.getAttribute("aria-label") ?? null';
const VIEW = '[...document.querySelectorAll("[role=tab]")].find((t) => t.getAttribute("aria-selected") === "true")?.textContent ?? null';

let server;
try {
  copyFileSync(join(repo, "fixtures/xer/batches/001/c-start-after.xer"), join(dir, "plant.xer"));
  totalfloat("history", "init", "plant.xer", "-m", "Start");
  // Enough rows to scroll.
  const wbs = totalfloat("query", store, "--columns", "code,wbs").split("\n")[1].split(/\s+/)[1];
  const adds = [];
  for (let k = 3; k <= 80; k++) adds.push("--add", `A${k};Synthetic activity ${k};${wbs};1d`);
  totalfloat("edit", store, ...adds, "-m", "More");
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
  check("the app loads", waitFor('!!document.querySelector("#grid-mirror tbody tr td + td")'));
  check("no menu bar renders", evaluate('document.querySelector("[role=menubar]") === null'));
  check(
    "the title block names the project, branch, data date, finish and float",
    evaluate(`(() => {
      const labels = [...document.querySelectorAll(".title-block .fact-label")].map((l) => l.textContent);
      return document.querySelector(".project-name").textContent.length > 0 && ["Branch", "Data date", "Finish", "Float"].every((l) => labels.includes(l));
    })()`),
    evaluate('[...document.querySelectorAll(".title-block .fact-label")].map((l) => l.textContent)'),
  );

  // By mouse, one action from each section.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  choose("File", "Reload");
  check("File > Reload reloads, and says so", waitFor(`(${NOTICE}) === "Reloaded the schedule."`), evaluate(NOTICE));
  check("and closes the menu", !open());
  check("focus goes back to the grid", evaluate('document.activeElement === document.querySelector(".grid-body")'), focused());
  const preview = evaluate('document.querySelector(".preview-toggle input").checked');
  choose("Edit", "Preview edits");
  check("Edit > Preview edits turns previews off", waitFor(`document.querySelector(".preview-toggle input").checked === ${!preview}`));
  choose("Edit", "Preview edits");
  choose("View", "Right column");
  check("View > Right column opens it", waitFor(`(${RIGHT}) !== null`));
  choose("View", "Right column");
  check("and closes it", waitFor(`(${RIGHT}) === null`));
  // Select an activity, then explain it from the Schedule section.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  cli("press", "ArrowDown");
  choose("Schedule", "Explain");
  check(
    "Schedule > Explain explains the selection, in the inspector",
    waitFor('!!document.querySelector(".right-column .inspector-code")?.textContent && document.querySelectorAll(".right-column .explain-stop").length > 0'),
  );
  evaluate('(document.querySelector(".explain-close").click(), true)');
  choose("Help", "Keyboard shortcuts");
  check("Help > Keyboard shortcuts lists the keys", waitFor('!!document.querySelector("[aria-label=\'Keyboard shortcuts\']")'));
  // Each key the help lists is in the guide's key table, whole: Alt+X isn't
  // found in Alt+Shift+X.
  const guide = readFileSync(join(repo, "docs/guide/serve.md"), "utf8");
  const table = guide.slice(guide.indexOf("## Keys"), guide.indexOf("\n## ", guide.indexOf("## Keys") + 1));
  const helpKeys = evaluate('[...document.querySelectorAll("[aria-label=\'Keyboard shortcuts\'] kbd")].map((k) => k.textContent)');
  const whole = (key) => new RegExp(`(?<![\\w+])${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\w+])`).test(table);
  check("and the guide's key table lists each of them", helpKeys.length > 20 && helpKeys.every(whole), helpKeys.filter((key) => !whole(key)));
  cli("press", "Escape");

  // By keyboard: Alt+Shift+M opens the Menu at its first item, arrows move, Enter runs.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+M");
  check("Alt+Shift+M opens the Menu at its first item", open() && /^Export XER/.test(focused() ?? ""), focused());
  cli("press", "Alt+Shift+M");
  check("and closes it again", !open());
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Alt+Shift+M");
  cli("press", "Escape");
  check("Escape closes it, and focus goes to the Menu button", !open() && focused() === "Menu", focused());
  cli("press", "ArrowDown");
  check("Down on the button opens it again", open());
  // Type-ahead: R goes to Reload, the first item after Export XER starting with it.
  cli("press", "r");
  check("type-ahead finds Reload", /^Reload/.test(focused() ?? ""), focused());
  cli("press", "Enter");
  check("Enter runs it", waitFor(`(${NOTICE}) === "Reloaded the schedule."`), evaluate(NOTICE));

  // The views: each from its tab and from its key; the schedule keeps its scroll and selection.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+Home");
  for (let k = 0; k < 40; k++) cli("press", "ArrowDown");
  const before = evaluate('({ top: document.querySelector(".grid-body").scrollTop, active: document.querySelector(".grid-body").getAttribute("aria-activedescendant") })');
  check("the grid scrolled to a row far down", before.top > 0, before);
  for (const [tab, key, region] of [
    ["Network", "Alt+Shift+W", ".view-network"],
    ["Resources", "Alt+Shift+U", "section[aria-label='Resource usage']"],
    ["Risk", "Alt+Shift+R", "section[aria-label='Schedule risk']"],
    ["History", "Alt+h", "section[aria-label='History']"],
  ]) {
    click("[role=tab]", tab);
    check(`${tab} opens from the title block`, waitFor(`(${VIEW}) === ${JSON.stringify(tab)} && !!document.querySelector(${JSON.stringify(region)})`), evaluate(VIEW));
    click("[role=tab]", "Schedule");
    check(`and Schedule comes back from its tab with the same scroll and selection`,
      waitFor(`(() => { const g = document.querySelector(".grid-body"); return g.scrollTop === ${before.top} && g.getAttribute("aria-activedescendant") === ${JSON.stringify(before.active)} && !document.querySelector(".view-page:not([hidden])"); })()`),
      evaluate('({ top: document.querySelector(".grid-body").scrollTop, active: document.querySelector(".grid-body").getAttribute("aria-activedescendant") })'));
    evaluate('(document.querySelector(".grid-body").focus(), true)');
    cli("press", key);
    check(`${tab} opens from ${key}`, waitFor(`(${VIEW}) === ${JSON.stringify(tab)} && !!document.querySelector(${JSON.stringify(region)})`), evaluate(VIEW));
    cli("press", "Alt+Shift+G");
    check("and Alt+Shift+G goes back to Schedule", waitFor(`(${VIEW}) === "Schedule" && !document.querySelector(".view-page:not([hidden])")`), evaluate(VIEW));
  }
  check("the schedule kept its scroll and selection throughout",
    evaluate(`(() => { const g = document.querySelector(".grid-body"); return g.scrollTop === ${before.top} && g.getAttribute("aria-activedescendant") === ${JSON.stringify(before.active)}; })()`));
  if (shot) {
    click(".menu-button button", "Menu");
    cli("screenshot", `--filename=${shot}`);
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
