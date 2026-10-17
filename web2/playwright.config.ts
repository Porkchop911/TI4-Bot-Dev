import { defineConfig, devices } from "@playwright/test";

// Two suites, each on the desktop and on a Pixel 9 in portrait. Every suite has one worker.
// `npm run e2e` (the projects "desktop" and "phone") plays the real game: the engine in the page
// and the server. It is the suite for a change. A test has 5 seconds; a test of a long flow sets
// 10 for itself.
// `npm run e2e:slow` (the projects "slow" and "slow-phone") has the tests of the real game with
// "@slow" in the title: the ones that play a game through. A test has 30 seconds.
// The slow tests are run by hand, not for each change.
const port = process.env.WEB2_PORT ?? "3190";
// The server of the online game (e2e/online.spec.ts). The page reaches it through the proxy of
// vite, which reads the port from the same variable.
const backendPort = process.env.TI4_E2E_BACKEND_PORT ?? "3191";
process.env.TI4_E2E_BACKEND_PORT = backendPort;

// The installed app has the whole screen of the phone, with no browser bars: 1080×2424 at the
// default display size of a Pixel 9. (The preset of Playwright assumes a larger one.)
const PHONE = {
  ...devices["Pixel 9"],
  viewport: { width: 412, height: 923 },
  deviceScaleFactor: 2.625,
};

const server = {
  name: "server",
  // `b-web` builds the server with the features of `cargo t-web`, so the two share one build of
  // the engine (`cargo run -p ti4-server` builds its own).
  command: "cargo b-web --quiet && exec target/debug/server",
  cwd: "..",
  // The banner is printed after the port is bound. The first build takes minutes.
  wait: { stdout: new RegExp(`Listening on:\\s+http://127\\.0\\.0\\.1:${backendPort}`) },
  timeout: 600_000,
  env: {
    PORT: backendPort,
    HOST: "127.0.0.1",
    // The games of a run are its own: nothing is recovered, and nothing is kept.
    TI4_DATA_DIR: `/tmp/ti4-web2-e2e-games-${backendPort}-${process.pid}`,
    // A seat that the server plays answers at once: by default it waits before each answer.
    TI4_RANDOM_BOT_DELAY_MS: "0",
    // Three offered models (the synthetic model of the tests under three names, `cargo wire-web`):
    // a lobby shows three models, and a game with an MLP seat is fast in a debug build.
    TI4_MODEL_DIR: "fixtures/models",
  },
};
const page = {
  name: "page",
  command: `exec npx vite --host 127.0.0.1 --port ${port} --strictPort`,
  wait: { stdout: new RegExp(`Local:\\s+http://127\\.0\\.0\\.1:${port}/`) },
  reuseExistingServer: true,
  timeout: 20_000,
};

const SLOW = /@slow/;
const PHONE_SPEC = /phone\.spec\.ts/;

export default defineConfig({
  testDir: "./e2e",
  // The machine that runs the suites is about half as fast as a desktop.
  timeout: 5_000,
  // One worker: the suite also runs on a machine with little memory (AGENTS.md, "Rust builds").
  workers: 1,
  reporter: "line",
  use: { baseURL: `http://127.0.0.1:${port}`, ...devices["Desktop Chrome"], deviceScaleFactor: 1 },
  projects: [
    { name: "desktop", testIgnore: PHONE_SPEC, grepInvert: SLOW },
    { name: "phone", testMatch: PHONE_SPEC, grepInvert: SLOW, use: PHONE },
    { name: "slow", testIgnore: PHONE_SPEC, grep: SLOW, timeout: 30_000 },
    { name: "slow-phone", testMatch: PHONE_SPEC, grep: SLOW, timeout: 30_000, use: PHONE },
  ],
  webServer: [server, page],
});
