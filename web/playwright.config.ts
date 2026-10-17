import { defineConfig, devices } from "@playwright/test";

// Give each invocation its own ports so local dev servers and concurrent runs do not collide.
const portBase = 20_000 + Math.floor(Math.random() * 30_000);
const backendPort = process.env.TI4_E2E_BACKEND_PORT ?? String(portBase);
const frontendPort = process.env.TI4_E2E_FRONTEND_PORT ?? String(portBase + 1);
process.env.TI4_E2E_BACKEND_PORT = backendPort;
process.env.TI4_E2E_FRONTEND_PORT = frontendPort;

// TI4_E2E_WEB=build serves a production bundle (vite build, cached per commit, then vite preview with
// the same /api and /ws proxy) instead of the vite dev server. Default: dev.
const webBuild = process.env.TI4_E2E_WEB === "build";

export default defineConfig({
  testDir: "./e2e",
  // The offline game needs the production build and no backend: `npm run test:e2e:offline`.
  testIgnore: /offline_local\.spec\.ts$/,
  timeout: 30_000,
  globalTimeout: 300_000,
  expect: {
    timeout: 5_000,
  },
  fullyParallel: false,
  workers: 2,
  reporter: "list",
  use: {
    baseURL: `http://127.0.0.1:${frontendPort}`,
    trace: "on-first-retry",
    // Playwright starts Chromium with --disable-dev-shm-usage, so its shared memory (data pipes,
    // compositor frames) lives in unlinked files under /tmp. When /tmp is a nearly full tmpfs every
    // page after the first fails with net::ERR_INSUFFICIENT_RESOURCES. The nightly sweep sets
    // TI4_E2E_DEV_SHM=1 (when /dev/shm is big enough) to keep the browser's default /dev/shm backing.
    launchOptions:
      process.env.TI4_E2E_DEV_SHM === "1" ? { ignoreDefaultArgs: ["--disable-dev-shm-usage"] } : {},
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
      // TI4_E2E_SERVER_BIN: run an already built server binary instead of letting cargo check (and
      // possibly rebuild) the workspace at every start.
      command: process.env.TI4_E2E_SERVER_BIN
        ? `exec ${process.env.TI4_E2E_SERVER_BIN}`
        : "cargo run --quiet -p ti4-server --bin server",
      cwd: "..",
      // This environment accepts unopened loopback connections without replying, so an HTTP
      // readiness probe never returns. The startup banner prints only after a successful bind.
      wait: { stdout: new RegExp(`Listening on:\\s+http://127\\.0\\.0\\.1:${backendPort}`) },
      timeout: 180_000,
      env: {
        PORT: backendPort,
        HOST: "127.0.0.1",
        // Never recover developer sessions while starting the bounded E2E server.
        TI4_DATA_DIR: process.env.TI4_E2E_DATA_DIR ?? `/tmp/ti4-playwright-games-${backendPort}`,
        TI4_DEV_PRESENCE_GRACE_MS: "1000",
      },
    },
    {
      name: "frontend",
      command: webBuild
        ? `dist=$(bash scripts/e2e-web-build.sh) && exec npx vite preview --outDir "$dist" --host 127.0.0.1 --port ${frontendPort} --strictPort`
        : `exec npx vite --host 127.0.0.1 --port ${frontendPort} --strictPort`,
      wait: { stdout: new RegExp(`Local:\\s+http://127\\.0\\.0\\.1:${frontendPort}/`) },
      timeout: webBuild ? 300_000 : 5_000,
      env: { VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS: "250" },
    },
  ],
});
