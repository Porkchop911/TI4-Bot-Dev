import { defineConfig, devices } from "@playwright/test";

// Give each invocation its own ports so local dev servers and concurrent runs do not collide.
const portBase = 20_000 + Math.floor(Math.random() * 30_000);
const backendPort = process.env.TI4_E2E_BACKEND_PORT ?? String(portBase);
const frontendPort = process.env.TI4_E2E_FRONTEND_PORT ?? String(portBase + 1);
process.env.TI4_E2E_BACKEND_PORT = backendPort;
process.env.TI4_E2E_FRONTEND_PORT = frontendPort;

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  globalTimeout: 120_000,
  expect: {
    timeout: 5_000,
  },
  fullyParallel: false,
  workers: 4,
  reporter: "list",
  use: {
    baseURL: `http://127.0.0.1:${frontendPort}`,
    trace: "on-first-retry",
    actionTimeout: 5_000,
    navigationTimeout: 5_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: [
    {
      name: "backend",
      command: "cargo run --quiet -p ti4-server --bin server",
      cwd: "..",
      // This environment accepts unopened loopback connections without replying, so an HTTP
      // readiness probe never returns. The startup banner prints only after a successful bind.
      wait: { stdout: new RegExp(`Listening on:\\s+http://127\\.0\\.0\\.1:${backendPort}`) },
      timeout: 180_000,
      env: {
        PORT: backendPort,
        HOST: "127.0.0.1",
        // Never recover developer sessions while starting the bounded E2E server.
        TI4_DATA_DIR: `/tmp/ti4-playwright-games-${backendPort}`,
        TI4_DEV_PRESENCE_GRACE_MS: "1000",
      },
    },
    {
      name: "frontend",
      command: `exec npx vite --host 127.0.0.1 --port ${frontendPort} --strictPort`,
      wait: { stdout: new RegExp(`Local:\\s+http://127\\.0\\.0\\.1:${frontendPort}/`) },
      timeout: 5_000,
      env: { VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS: "250" },
    },
  ],
});
