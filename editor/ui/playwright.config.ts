import { defineConfig } from "@playwright/test";

// Browser-mode end-to-end tests: the built UI in Chromium against a real headless engine
// (streamed viewport, ADR-0043 approach D). Needs `cargo build -p concordia-cli` first.
export default defineConfig({
  testDir: "tests/e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  // CI's virtual GPU renders the browser viewport's frames slowly, and they block the engine thread
  // (later calls wait behind them; see the M1 plan's gate notes), so assertions wait longer there.
  expect: { timeout: process.env["CI"] ? 15_000 : 5_000 },
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:4173", browserName: "chromium", headless: true },
  webServer: {
    command: "pnpm exec vite preview --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
  },
});
