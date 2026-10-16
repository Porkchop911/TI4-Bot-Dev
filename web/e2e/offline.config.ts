import { defineConfig, devices } from "@playwright/test";

// The offline game against the PRODUCTION build only: no backend, no dev server. The webServer
// builds the engine (scripts/build-wasm.mjs; TI4_WASM_FILE reuses a prebuilt file), runs
// `vite build` into .e2e-dist-offline and serves exactly those files with `vite preview`.
//   cd web && npm run test:e2e:offline
const port = process.env.TI4_E2E_OFFLINE_PORT ?? String(20_000 + Math.floor(Math.random() * 30_000));
process.env.TI4_E2E_OFFLINE_PORT = port;

export default defineConfig({
  testDir: ".",
  testMatch: /offline_local\.spec\.ts$/,
  outputDir: process.env.TI4_E2E_OUTPUT ?? "../test-results",
  timeout: 180_000,
  workers: 1,
  fullyParallel: false,
  reporter: "list",
  use: { baseURL: `http://127.0.0.1:${port}`, actionTimeout: 10_000, navigationTimeout: 15_000 },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    name: "offline-build",
    cwd: "..",
    command:
      "TI4_REQUIRE_WASM=1 node scripts/build-wasm.mjs && npx vite build --outDir .e2e-dist-offline --emptyOutDir && " +
      `exec npx vite preview --outDir .e2e-dist-offline --host 127.0.0.1 --port ${port} --strictPort`,
    wait: { stdout: new RegExp(`Local:\\s+http://127\\.0\\.0\\.1:${port}/`) },
    timeout: 600_000,
  },
});
