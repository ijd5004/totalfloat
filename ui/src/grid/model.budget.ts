// The grid's wall-clock budgets on 50,000 activities. They only
// hold on a quiet machine, so they run apart from the unit tests:
// `pnpm run budget` in `ui/`, which `cargo xtask ui-bench` also runs. The unit
// tests in `model.test.ts` time the same work against a reference task
// instead, which load doesn't change.

import { describe, expect, it } from "vitest";

import { buildModel } from "./model";
import { AT_SCALE, SCALE, synthetic, type Work } from "./model.fixture";

const ROWS = synthetic(SCALE);

/** The most each piece of work may take, in milliseconds. */
const BUDGET_MS: Record<Work, number> = {
  filter: 100,
  sortTwoKeys: 100,
  wbs: 100,
  phase: 100,
  wbsThenPhase: 100,
  phaseThenWbs: 100,
  roomThenTag: 300,
  roomThenWbs: 300,
};

/**
 * The best of five runs of `work`, each on a model built afresh outside the
 * timing, so nothing worked out by an earlier run (such as sort ranks) is
 * reused, while a slow first run from warming up doesn't count.
 */
function best(work: (typeof AT_SCALE)[Work]): number {
  let fastest = Infinity;
  for (let run = 0; run < 5; run++) {
    const model = buildModel(ROWS);
    const start = performance.now();
    work(model);
    fastest = Math.min(fastest, performance.now() - start);
  }
  return fastest;
}

describe("on 50,000 activities, on a quiet machine", () => {
  for (const name of Object.keys(BUDGET_MS) as Work[]) {
    const budget = BUDGET_MS[name];
    it(`${name} takes under ${budget} ms`, () => {
      const ms = best(AT_SCALE[name]);
      console.log(`${name}: ${ms.toFixed(1)} ms (budget ${budget} ms)`);
      expect(ms).toBeLessThan(budget);
    });
  }
});
