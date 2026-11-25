# TotalFloat web app

The local app that `totalfloat serve` shows ([ARCHITECTURE.md, Local app](../docs/ARCHITECTURE.md#local-app)): TypeScript and React, built with Vite.
The built files are compiled into the `totalfloat` binary, so a release is still one file and
nothing is fetched at run time. A Rust-only build works without any of this and serves a page
saying how to build the app.

## Building

You need Node.js (the version in `.node-version`) and [pnpm](https://pnpm.io). Then, from the
repository root:

```text
cargo xtask ui              # pnpm install --frozen-lockfile, then build into ui/dist/
cargo build --release       # compiles ui/dist/ into the binary
cargo xtask ui --check      # type check, lint and tests
cargo xtask ui --licenses   # refuse production dependencies deny.toml doesn't allow
```

`just check` runs `cargo xtask ui --check` when pnpm is on the PATH, and CI always does.

The binary embeds whatever `ui/dist/` holds when it's built, so run `cargo xtask ui` again after
pulling or switching to changes in `ui/`.

To see a change, run `cargo xtask ui` again, then `cargo run -- serve` on some schedule. The
server checks the host, the session and the origin of every call, so the app is always tried as
the binary serves it.

## How it fits together

- `src/api/`: the client for every `/api/1/` endpoint. `pnpm gen` writes `src/api/schemas/` from
  `docs/schemas/`; don't edit or commit it. `src/api/golden/` holds real answers from the server,
  which `cargo test` keeps current; after a deliberate change to an answer, rewrite them with
  `TOTALFLOAT_UPDATE_CONTRACT=1 cargo test -p totalfloat-cli golden`.
- `src/events.ts`: the live change feed, with reconnects and the connection indicator.
- `src/commands.ts`: register every action once, with its keys. The command palette and the key
  list both read the registry, so a key can't be bound twice or be missing from either.
- `src/shell/`: the frame every screen sits in: the title block (project picker, views, branch,
  data date, finish and float), the Menu button (built from the command registry; `node
  e2e/menus.mjs` checks it and the views in a browser), the toolbar row the grid puts its filters
  in, the schedule's right column, and the status bar. Icons are inline SVG in `icons.tsx`. The
  design tokens are at the top of `src/styles.css`; use them rather than new colours.
- `src/grid/`: the activity grid: the row model (`model.ts`, shared with the Gantt), the canvas
  drawing, and the component. `node e2e/grid.mjs [--bin target/release/totalfloat]` checks it in a
  browser through playwright-cli, on a schedule it builds from a synthetic fixture: WBS bands,
  more lines than Page Down moves, and months long, so the Gantt can zoom in. `node e2e/grid.mjs
  '<sign-in link>' [--timing]` runs it on a schedule you serve instead, which needs the same;
  for timing, serve the 50k synthetic one that `cargo xtask ui-bench` writes.
- `src/gantt/`: the Gantt chart, drawn on a canvas from the grid's row model (`src/view.ts` shares
  it), with the time axis in `time.ts` and logic lines in `lines.ts`. `e2e/grid.mjs --timing`
  measures scrolling with it drawn; `node e2e/gantt.mjs` checks the driving path against the
  engine's, on a store it builds.
- `src/edit/`: editing in the grid: typed cells into edit commands (`cells.ts`), and the preview,
  record, refusal, undo and moved-branch flow (`editing.ts`). `node e2e/edit.mjs` builds its own
  store from a fixture, serves it with `target/release/totalfloat` (or `--bin`), and checks
  editing end to end.
- `src/history/`: the History panel (log with its graph, `graph.ts`; revert, branch and baseline from a change, export,
  compare) and its words; the branch picker is `src/shell/BranchPicker.tsx`. `node
  e2e/history.mjs` builds its own store and checks branching, comparing, baselines, revert and
  export in a browser, and `node e2e/history-graph.mjs [--screenshot FILE]` the graph of branches, a
  merge and a proposal. `src/review/` is the Review panel for proposals; `node e2e/review.mjs` runs
  a second process as an agent proposing edits, and checks accepting, rejecting, replaying and
  conflicts. `node e2e/merge.mjs` checks a clean merge and one refused for a clash.
- Layouts: `src/shell/layouts.ts` converts what's shown to and from `totalfloat.layout/1`, and
  `src/shell/LayoutDialog.tsx` saves, opens and deletes them. `node e2e/layouts.mjs` saves one,
  reloads, and checks the view comes back.
- `src/print/`: the print view, SVG pages of the grid and Gantt (`pages.ts` is the geometry).
  `node e2e/print.mjs` prints 502 activities and checks the pages and the PDF's text layer;
  it needs `pdftotext` and `pdfinfo` (poppler).
- The e2e scripts drive whichever browser playwright-cli's config names. To run one in Firefox
  or another Chromium, point `PLAYWRIGHT_MCP_CONFIG` at a config file such as
  `{"browser": {"browserName": "firefox"}}`; a machine-wide config's `executablePath` still
  applies, so name the browser's executable there too. Keys such as Ctrl+C go through the
  browser's own handling, so a clipboard check pressed as keys can pass in one browser and fail
  in another.
- `node e2e/bench.mjs '<sign-in link>'` times first paint, scrolling, an edit, a branch switch and
  opening a proposal; `cargo xtask ui-bench` runs it on a 50k schedule (see `docs/PERFORMANCE.md`).

## Rules

- **Nothing from another host.** No CDN scripts, fonts or icons: the app must work air-gapped,
  and the server's Content-Security-Policy allows only its own scripts and styles. The page
  uses the system's fonts, and any icon is a file in `public/` or `src/`.
- **Dependency install scripts stay off.** pnpm doesn't run the install scripts of
  dependencies unless they're listed in `onlyBuiltDependencies`; keep that list empty.
- **Permissive licenses only.** Production dependencies must be licensed as `deny.toml`
  allows ([ARCHITECTURE.md, License and dependencies](../docs/ARCHITECTURE.md#license-and-dependencies)); `cargo xtask ui --licenses` checks, in CI too.
- **A committed lockfile.** Change dependencies with `pnpm add`, and commit
  `pnpm-lock.yaml` with them.
