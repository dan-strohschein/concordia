import { defineConfig } from "@playwright/test";

// Browser-mode end-to-end tests: the built UI in Chromium against a real headless engine
// (streamed viewport, ADR-0043 approach D). Needs `cargo build -p concordia-cli` first.
export default defineConfig({
  testDir: "tests/e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:4173", browserName: "chromium", headless: true },
  webServer: {
    command: "pnpm exec vite preview --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
  },
});
