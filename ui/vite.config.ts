import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The server's Content-Security-Policy allows only same-origin scripts and
// styles, so the build must not inline any: no module preload polyfill, and
// no assets turned into data: URLs except images, which the policy allows.
export default defineConfig({
  plugins: [react()],
  test: {
    // One file at a time: the grid's timing tests on 50,000 rows mustn't
    // compete for the CPU with other test files.
    fileParallelism: false,
    projects: [
      // `pnpm test`: the unit tests.
      { extends: true, test: { name: "unit" } },
      // `pnpm run budget`: wall-clock budgets, which hold only on a quiet
      // machine (src/grid/model.budget.ts).
      { extends: true, test: { name: "budget", include: ["src/**/*.budget.ts"] } },
    ],
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    modulePreload: { polyfill: false },
    assetsInlineLimit: (file) => (/\.(png|svg|gif|jpe?g)$/.test(file) ? undefined : false),
  },
});
