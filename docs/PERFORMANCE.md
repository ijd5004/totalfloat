# Performance

How fast TotalFloat is on a large schedule, how to measure it, and the targets.

## Measuring

`cargo xtask ui-bench` measures the app:

1. It writes a 50,000-activity synthetic schedule: chains of 100 finish-to-start links, each under its own WBS node, joined end to end in runs of ten chains (1,000 activities).
2. It starts a history store, adds a `what-if` branch and an agent's proposal, and serves the store.
3. It drives Chromium through `ui/e2e/bench.mjs`.

Each action is started inside the page and timed there, so playwright-cli's own latency isn't counted. The bench needs Node.js, pnpm and playwright-cli, and runs locally; CI doesn't run it.

```
cargo xtask ui-bench [--activities 50000]
```

The engine and checks have their own benches. CI's weekly bench job runs the engine's and reports without failing.

- `cargo bench -p totalfloat-engine --bench schedule`: one CPM pass (`pass`), seeded networks of 1,000 to 100,000 activities (`schedule`), and recalculating after an edit (`reuse`).
- `cargo bench -p totalfloat-check --bench assess`: the quality checks on the 50,000-activity schedule.

Results depend on load, so compare builds by running them in turns on the same machine.

## Grid model budgets

The grid's filtering, sorting and grouping on 50,000 activities have wall-clock budgets: 100 ms each, and 300 ms for grouping by a field of 20,000 values with a second level under it. `ui/src/grid/model.budget.ts` checks them; run `pnpm run budget` in `ui/`. `cargo xtask ui-bench` runs it first and reports whether they were met. They hold only on a quiet machine, so they're kept out of the unit tests.

The unit tests in `ui/src/grid/model.test.ts` time the same work against a reference task instead: plain JavaScript that groups and sorts 50,000 strings. The test checks the median of seven ratios of the work to the reference. Load slows both halves alike, so the ratio holds under load, and a slower grid raises it.

## Targets and latest results

Measured on 2026-11-22 on an Intel Core i7-7500U (2 cores, 4 threads), 16 GB, Linux, headless Chromium, at 50,000 activities:

| Measure | Latest | Target | Met |
|---|---|---|---|
| First paint, from navigation to the grid's first paint | 0.30 s | under 3 s | yes |
| Grid scroll, plain and grouped two levels deep: average / 95th percentile frame | 16.7 / 16.8 ms | 17 / 33 ms | yes |
| Gantt scroll with logic lines on: average / 95th percentile frame | 16.7 / 16.7 ms | 17 / 33 ms | yes |
| Edit preview shown | 0.79 s | under 300 ms | **no** |
| Edit recorded, view refreshed | 2.53 s | under 1 s | **no** |
| Branch switched and shown | about 2 s | none set | |
| Proposal opened, its comparison shown | about 1 s | none set | |

**Frame times** are the gaps between animation frames, which the browser paces to the display's refresh. A page that never drops a frame measures 16.7 ms at 60 Hz, so the average target is one frame, with 0.3 ms allowed for timer jitter. The grid scrolls two screens a frame through all 50,501 rows; the Gantt scrolls half a screen a frame across time and back. Under heavy load the plain grid scroll sits at its target with a frame or two dropped.

**Engine and checks:** one CPM pass over the 50,000 activities takes about 135 ms. Recalculating after an edit near the end of a path takes about 98 ms, against about 140 ms for a full calculation. The quality checks take about 350 ms, and about 210 ms in an edit preview, which reuses the calculation it has just made.

## Where an edit's time goes

The bench's edit lengthens the first activity, which changes the dates and float of all 50,000 activities, so it's the worst case for an edit.

- **A preview** is answered from the view `serve` already holds. It applies the edit, recalculates the activities the edit reaches (`engine::schedule_reusing`), assesses the quality checks on the edited schedule, and compares before and after. The checks are the largest part.
- **A recorded edit** also writes the edited file with its new results and records the snapshot. The view after it is built from the edit's own calculation where no project links to another, so it isn't calculated again. The page then fetches and paints the new view, about 0.9 s.

Each of these shortcuts gives the same answer as the full path. `crates/engine/tests/reuse.rs`, `crates/cli/tests/serve.rs` and the `corpus_reuse` and `corpus_assess` tests compare them, field by field, with a full calculation.

**WebAssembly isn't the fix.** Nearly all of an edit's time is server work. A browser running the same calculation would be slower than the native server. Revisit it if a preview's server time comes under about 300 ms and the round trip then dominates ([ARCHITECTURE.md, Local app](ARCHITECTURE.md#local-app)).
