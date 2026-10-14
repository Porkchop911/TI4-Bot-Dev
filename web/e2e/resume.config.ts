import { defineConfig, devices } from "@playwright/test";

// Resume-after-idle against a REAL backend that the spec itself starts, kills and restarts (the
// normal config's backend is owned by Playwright and cannot be restarted from a test). Only vite is
// started here; its proxy points at TI4_E2E_BACKEND_PORT. Needs a built server binary:
//   TI4_SERVER_BIN=../target/release/server npm run test:e2e:resume
const backendPort = process.env.TI4_E2E_BACKEND_PORT ?? "43310";
const frontendPort = process.env.TI4_E2E_FRONTEND_PORT ?? "43311";
process.env.TI4_E2E_BACKEND_PORT = backendPort;
process.env.TI4_E2E_FRONTEND_PORT = frontendPort;
process.env.TI4_RESUME_REAL = "1";

export default defineConfig({
  testDir: ".",
  testMatch: /resume_real\.spec\.ts$/,
  outputDir: process.env.TI4_E2E_OUTPUT ?? "../test-results",
  timeout: 120_000,
  workers: 1,
  fullyParallel: false,
  reporter: "list",
  use: {
    baseURL: `http://127.0.0.1:${frontendPort}`,
    actionTimeout: 10_000,
    navigationTimeout: 15_000,
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    name: "frontend",
    cwd: "..",
    command: `exec npx vite --config e2e/screenshots/_shared/vite.shots.config.ts --host 127.0.0.1 --port ${frontendPort} --strictPort`,
    wait: { stdout: new RegExp(`Local:\\s+http://127\\.0\\.0\\.1:${frontendPort}/`) },
    timeout: 30_000,
    env: { VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS: "1000" },
  },
});
