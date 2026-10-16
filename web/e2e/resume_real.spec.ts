import { expect, test, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { createStartedGame, gameSnapshot, openPlayerGame } from "./lobbyHelpers";

// A phone that slept while the backend (behind an SSH tunnel) went away: the first fetches fail,
// the websocket is dead. Against the real server binary: kill it, "sleep" 30 minutes on the page
// clock, wake, restart it (games recover from disk) and expect a silent resume. Also a server
// that comes back WITHOUT the game (data wiped): a clear message, not a TypeError.
// Run with `npm run test:e2e:resume`; skipped under the normal config (it owns the backend there).
test.skip(!process.env.TI4_RESUME_REAL, "run with npm run test:e2e:resume");

const port = process.env.TI4_E2E_BACKEND_PORT ?? "43310";
const bin = resolve(process.env.TI4_SERVER_BIN ?? "../target/release/server");
const dataDir = resolve(process.env.TI4_RESUME_DATA ?? `./test-results/resume-data-${port}`);

let server: ChildProcess | null = null;

async function startServer(): Promise<void> {
  if (!existsSync(bin)) throw new Error(`server binary not found: ${bin} (set TI4_SERVER_BIN)`);
  mkdirSync(dataDir, { recursive: true });
  const child = spawn(bin, [], {
    env: { ...process.env, PORT: port, HOST: "127.0.0.1", TI4_DATA_DIR: dataDir, TI4_DEV_PRESENCE_GRACE_MS: "1000" },
    stdio: ["ignore", "pipe", "inherit"],
  });
  server = child;
  await new Promise<void>((done, fail) => {
    const timer = setTimeout(() => fail(new Error("server did not start")), 60_000);
    child.stdout!.on("data", (chunk: Buffer) => {
      if (new RegExp(`Listening on:\\s+http://127\\.0\\.0\\.1:${port}`).test(String(chunk))) {
        clearTimeout(timer);
        done();
      }
    });
    child.on("exit", (code) => fail(new Error(`server exited early (${code})`)));
  });
}

async function stopServer(): Promise<void> {
  const child = server;
  server = null;
  if (!child || child.exitCode !== null) return;
  await new Promise<void>((done) => {
    child.once("exit", () => done());
    child.kill("SIGKILL");
  });
}

test.afterEach(async () => {
  await stopServer();
});

const sleepAndWake = async (page: Page, awayMs: number, whileHidden: () => Promise<void>) => {
  const set = (value: "hidden" | "visible") =>
    page.evaluate((state) => {
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
      document.dispatchEvent(new Event("visibilitychange"));
    }, value);
  await set("hidden");
  await whileHidden();
  await page.clock.fastForward(awayMs);
  await set("visible");
};

const runClock = async (page: Page, ms: number) => {
  for (let spent = 0; spent < ms; spent += 1_000) {
    await page.clock.runFor(1_000);
    await page.waitForTimeout(30);
  }
};

const noRawError = async (page: Page) => {
  await expect(page.locator(".session-error")).toHaveCount(0);
  await expect(page.getByText(/Failed to fetch|TypeError|Check the lobby/i)).toHaveCount(0);
};

test("the backend restarts while the tab sleeps: silent resume with the game recovered from disk", async ({ page, request }) => {
  rmSync(dataDir, { recursive: true, force: true });
  await startServer();
  const { gameId, players } = await createStartedGame(request, 2, 4242);
  await page.clock.install();
  await openPlayerGame(page, gameId, players[0].session);
  await expect(page.locator('[data-status="connected"]').first()).toBeVisible({ timeout: 20_000 });
  const before = await gameSnapshot(request, gameId, players[0].session);

  await sleepAndWake(page, 30 * 60_000, () => stopServer());
  await expect(page.getByTestId("lobby-connection-lost")).toBeVisible({ timeout: 10_000 });
  await noRawError(page);

  await startServer();
  await runClock(page, 60_000);
  await expect(page.getByTestId("lobby-connection-lost")).toHaveCount(0, { timeout: 10_000 });
  await expect(page.getByTestId("resume-fatal")).toHaveCount(0);
  await expect(page.locator('[data-status="connected"]').first()).toBeVisible();
  await noRawError(page);
  const after = await gameSnapshot(request, gameId, players[0].session);
  expect(after.game_version).toBe(before.game_version);
  await expect(page.getByText(new RegExp(`v${after.game_version}\\b`)).first()).toBeVisible();
});

test("the server comes back without the game: a clear message with a way out", async ({ page, request }) => {
  rmSync(dataDir, { recursive: true, force: true });
  await startServer();
  const { gameId, players } = await createStartedGame(request, 2, 4243);
  await page.clock.install();
  await openPlayerGame(page, gameId, players[0].session);
  await expect(page.locator('[data-status="connected"]').first()).toBeVisible({ timeout: 20_000 });

  await sleepAndWake(page, 60 * 60_000, async () => {
    await stopServer();
    rmSync(dataDir, { recursive: true, force: true });
    await startServer();
  });
  const panel = page.getByTestId("resume-fatal");
  await expect(panel).toBeVisible({ timeout: 30_000 });
  await expect(panel).toContainText(/not found/i);
  await expect(panel).not.toContainText(/Failed to fetch|TypeError/i);
  await expect(page.getByTestId("resume-back")).toBeVisible();
});

test("the backend is down for good: a plain message with Retry, which works once it is back", async ({ page, request }) => {
  rmSync(dataDir, { recursive: true, force: true });
  await startServer();
  const { gameId, players } = await createStartedGame(request, 2, 4244);
  await page.clock.install();
  await openPlayerGame(page, gameId, players[0].session);
  await expect(page.locator('[data-status="connected"]').first()).toBeVisible({ timeout: 20_000 });
  await stopServer();
  await runClock(page, 150_000);
  const panel = page.getByTestId("resume-fatal");
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("Cannot reach the server");
  await expect(panel).not.toContainText(/Failed to fetch|TypeError|lobby/i);
  await startServer();
  await page.getByTestId("resume-retry").click();
  await runClock(page, 10_000);
  await expect(page.getByTestId("resume-fatal")).toHaveCount(0);
  await expect(page.locator('[data-status="connected"]').first()).toBeVisible();
});
