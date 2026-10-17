import { defineConfig, devices } from "@playwright/test";

// The offline sweep game against an already served production build (no webServer here: the
// runner, tools/nightly-smoke/offline-run.sh, builds it and starts `vite preview` once for all
// workers). TI4_OFFLINE_URL is that server, e.g. http://127.0.0.1:34567.
//   cd web && TI4_OFFLINE_URL=... npm run test:e2e:offline-sweep
export default defineConfig({
  testDir: ".",
  testMatch: /offline_sweep\.spec\.ts$/,
  outputDir: process.env.TI4_OFFLINE_PW_OUTPUT ?? "../test-results/offline-sweep-pw",
  timeout: 0,
  workers: 1,
  fullyParallel: false,
  reporter: "line",
  use: {
    baseURL: process.env.TI4_OFFLINE_URL ?? "http://127.0.0.1:4173",
    actionTimeout: 10_000,
    navigationTimeout: 30_000,
    // The engine in the page needs /dev/shm-sized memory for the wasm; same switch as the smoke runs.
    launchOptions: process.env.TI4_E2E_DEV_SHM === "1" ? { ignoreDefaultArgs: ["--disable-dev-shm-usage"] } : {},
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
