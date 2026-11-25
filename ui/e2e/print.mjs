// Printing, in a real browser, through playwright-cli: a 500-activity
// schedule made from a synthetic fixture goes to the print view, which splits
// it into pages of whole rows with the headers repeated, and Chromium's PDF
// of it has a text layer holding the activity names and the title block, so
// it's vector, not a picture.
//
//   cargo xtask ui && cargo build --release -p totalfloat-cli
//   node ui/e2e/print.mjs [--bin target/release/totalfloat] [--pdf out.pdf] [--screenshot print.png]
//
// Needs `pdftotext` (poppler) on the PATH. Exits non-zero if a check fails.
import { execFileSync, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = process.argv.slice(2);
const binAt = flags.indexOf("--bin");
const bin = resolve(binAt >= 0 ? flags[binAt + 1] : join(repo, "target/release/totalfloat"));
const shotAt = flags.indexOf("--screenshot");
const shot = shotAt >= 0 ? resolve(flags[shotAt + 1]) : null;
const session = `print-e2e-${process.pid}`;

const dir = mkdtempSync(join(tmpdir(), "totalfloat-print-e2e-"));
const store = join(dir, "plant.tfdb");

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

const pdfAt = flags.indexOf("--pdf");
const pdf = pdfAt >= 0 ? resolve(flags[pdfAt + 1]) : join(dir, "print.pdf");

let server;
try {
  // A store: the fixture, and 500 more activities in its WBS.
  copyFileSync(join(repo, "fixtures/xer/batches/001/c-start-after.xer"), join(dir, "plant.xer"));
  totalfloat("history", "init", "plant.xer", "-m", "Start");
  const wbs = totalfloat("query", store, "--columns", "code,wbs").split("\n")[1].split(/\s+/)[1];
  const adds = [];
  for (let k = 1; k <= 500; k++) adds.push("--add", `P${k};Printed activity ${k};${wbs};${(k % 5) + 1}d`);
  totalfloat("edit", store, ...adds, "-m", "Five hundred more");

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

  // Print, from the palette.
  evaluate('(document.querySelector(".grid-body").focus(), true)');
  cli("press", "Control+k");
  cli("type", "Print");
  cli("press", "Enter");
  check("the print view opens", waitFor('document.querySelectorAll(".print-page").length > 0'));
  const pages = evaluate(`(() => {
    return [...document.querySelectorAll(".print-page")].map((page) => {
      const height = Number(page.getAttribute("viewBox").split(" ")[3]);
      const ys = [...page.querySelectorAll(".print-row")].map((row) => {
        const box = row.getBoundingClientRect();
        return [box.top, box.bottom];
      });
      const pageBox = page.getBoundingClientRect();
      return {
        height,
        rows: ys.length,
        header: page.querySelector(".print-header").textContent.includes("Activity ID"),
        title: page.querySelector(".print-title").textContent,
        lowest: Math.max(...ys.map((y) => y[1])),
        footerTop: page.querySelector(".print-footer").getBoundingClientRect().top,
        bottom: pageBox.bottom,
      };
    });
  })()`);
  check("502 activities and their band split into pages", pages.length > 1 && pages.reduce((n, p) => n + p.rows, 0) === 503, pages.map((p) => p.rows));
  check("each page repeats the column headers", pages.every((p) => p.header), pages[0]);
  check("each page has the title block", pages.every((p) => p.title.includes("Printed") && p.title.includes("main @")), pages[0]?.title);
  check("no row runs into the footer or off the page", pages.every((p) => p.lowest <= p.footerTop && p.lowest <= p.bottom), pages.map((p) => [p.lowest, p.footerTop]));

  // Chromium's PDF of it, as print to PDF gives it.
  cli("run-code", `async (page) => { await page.emulateMedia({ media: "print" }); await page.pdf({ path: ${JSON.stringify(pdf)}, preferCSSPageSize: true, printBackground: true }); }`);
  check("a PDF is written", existsSync(pdf));
  const text = execFileSync("pdftotext", ["-layout", pdf, "-"], { encoding: "utf8" });
  check("its text layer holds the activity names", text.includes("Printed activity 1") && text.includes("Printed activity 500") && text.includes("Synthetic activity 2"));
  check("and the title block", /Printed \d\d [A-Z][a-z]{2} \d\d/.test(text) && text.includes("main @") && text.includes("Data date"));
  const info = execFileSync("pdfinfo", [pdf], { encoding: "utf8" });
  const pdfPages = Number(/Pages:\s+(\d+)/.exec(info)?.[1]);
  check("one PDF page per printed page", pdfPages === pages.length, { pdfPages, pages: pages.length });
  // Letter, landscape, as the page setup says by default: 792 by 612 points.
  check("on the paper the setup chose", /Page size:\s+792 x 612 pts/.test(info), /Page size:.*/.exec(info)?.[0]);
  cli("run-code", "async (page) => { await page.emulateMedia({ media: null }); }");

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
