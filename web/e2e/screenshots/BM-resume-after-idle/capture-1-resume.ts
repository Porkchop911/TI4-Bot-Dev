import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openResumableGame, sleepAndWake, type ResumableGame } from "../_shared/mockResumable";
import { actor, systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// A phone tab that slept (minutes to hours) comes back: the websocket is dead, the tunnel may have
// dropped, and the first fetches fail with "TypeError: Failed to fetch". The page must resume by
// itself: a small 'Reconnecting' chip while it works, the fresh server state afterwards, and for a
// server that stays away a plain message with Retry. TI4_SHOT_PREFIX=before is how the "before"
// shots were taken (on the base commit phone-play-2026-10-08, where the assertions do not hold and
// are skipped); the default "after" regenerates the current behaviour and asserts it.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const after = prefix === "after";
const MIN = 60_000;

const viewports = [
  { id: "desktop", width: 1440, height: 900 },
  { id: "phone", width: 390, height: 844 },
];

const choice = (prompt: string, nonce: string) => ({
  prompt,
  nonce,
  context: { subtype: "system_activation" },
  options: systemActivationOptions,
});
const serverState = (prompt: string, nonce: string, version: number) => ({
  seat: actor,
  players: [playerWithHand(), opponent],
  version,
  choice: choice(prompt, nonce),
});

async function open(page: Page): Promise<ResumableGame> {
  await page.clock.install();
  const game = await openResumableGame(page, serverState("Pick the system to activate", "nonce-1", 40));
  await expect(page.getByText("Pick the system to activate").first()).toBeVisible();
  await expect.poll(() => game.socket !== null).toBe(true);
  return game;
}

/** Lets the (installed) page clock run, so backoff timers and the chip's delay fire. */
async function runClock(page: Page, ms: number) {
  for (let spent = 0; spent < ms; spent += 1_000) {
    await page.clock.runFor(1_000);
    await page.waitForTimeout(30);
  }
}

const noErrorBanner = async (page: Page) => {
  await expect(page.locator(".session-error")).toHaveCount(0);
  await expect(page.getByText(/Failed to fetch|TypeError/i)).toHaveCount(0);
  await expect(page.getByText(/Check the lobby/i)).toHaveCount(0);
};

for (const vp of viewports) {
  test.describe(`${vp.id}`, () => {
    test.use({ viewport: { width: vp.width, height: vp.height } });

    test(`${vp.id}: 30 minutes asleep, tunnel down, then back: silent resume with fresh state`, async ({ page }, testInfo) => {
      test.setTimeout(90_000);
      const game = await open(page);
      await shot(page, testInfo, `${prefix}-${vp.id}-1-normal`);
      const heartbeats = game.counts.heartbeats;
      // The tunnel dropped while the phone slept, and someone else acted meanwhile.
      await sleepAndWake(page, 30 * MIN, () => {
        game.down = true;
        game.dropSocket();
        game.setServerState(serverState("Opponent acted: pick the next system", "nonce-2", 47));
      });
      if (after) {
        await expect(page.getByTestId("lobby-connection-lost")).toBeVisible({ timeout: 10_000 });
        await expect(page.getByTestId("lobby-connection-lost")).toHaveText(/Reconnecting/);
        await noErrorBanner(page);
        await expect(page.getByTestId("resume-fatal")).toHaveCount(0);
        // The last known game stays on screen while it reconnects.
        await expect(page.getByText("Pick the system to activate").first()).toBeVisible();
      } else await page.waitForTimeout(2_500);
      await shot(page, testInfo, `${prefix}-${vp.id}-2-reconnecting`);
      // The network comes back.
      game.down = false;
      if (after) {
        await runClock(page, 20_000);
        await expect(page.getByTestId("lobby-connection-lost")).toHaveCount(0);
        await expect(page.getByText("Opponent acted: pick the next system").first()).toBeVisible();
        await noErrorBanner(page);
        expect(game.counts.heartbeats).toBeGreaterThan(heartbeats);
      } else await page.waitForTimeout(3_000);
      await shot(page, testInfo, `${prefix}-${vp.id}-3-resumed`);
    });

    test(`${vp.id}: slow reconnect (five failed attempts, then it works) stays silent`, async ({ page }) => {
      test.skip(!after);
      test.setTimeout(90_000);
      const game = await open(page);
      game.failSockets = 5;
      game.failSnapshots = 5;
      game.dropSocket();
      await expect(page.getByTestId("lobby-connection-lost")).toBeVisible({ timeout: 10_000 });
      await noErrorBanner(page);
      await runClock(page, 60_000);
      await expect(page.getByTestId("lobby-connection-lost")).toHaveCount(0);
      await expect(page.getByTestId("resume-fatal")).toHaveCount(0);
      await noErrorBanner(page);
      expect(game.counts.sockets).toBeGreaterThanOrEqual(6);
    });

    test(`${vp.id}: a proxy 502 on the snapshot is retried like a network error`, async ({ page }) => {
      test.skip(!after);
      test.setTimeout(60_000);
      const game = await open(page);
      game.snapshotStatus = 502;
      game.dropSocket();
      await page.clock.runFor(3_000);
      await noErrorBanner(page);
      game.snapshotStatus = null;
      await runClock(page, 20_000);
      await expect(page.getByTestId("lobby-connection-lost")).toHaveCount(0);
      await noErrorBanner(page);
    });

    test(`${vp.id}: a server that stays away ends in a plain message with Retry`, async ({ page }, testInfo) => {
      test.setTimeout(120_000);
      const game = await open(page);
      game.down = true;
      game.dropSocket();
      if (after) {
        await runClock(page, 150_000);
        const panel = page.getByTestId("resume-fatal");
        await expect(panel).toBeVisible();
        await expect(panel).toContainText("Cannot reach the server");
        await expect(panel).not.toContainText(/Failed to fetch|TypeError|lobby/i);
        await expect(page.getByTestId("resume-retry")).toBeVisible();
        await expect(page.getByTestId("resume-back")).toBeVisible();
        await expect(page.getByTestId("lobby-connection-lost")).toHaveCount(0);
      } else await page.waitForTimeout(8_000);
      await shot(page, testInfo, `${prefix}-${vp.id}-4-gave-up`);
      if (!after) return;
      // Retry while it is still down: tries again, then asks again.
      await page.getByTestId("resume-retry").click();
      await expect(page.getByTestId("resume-fatal")).toHaveCount(0);
      await runClock(page, 150_000);
      await expect(page.getByTestId("resume-fatal")).toBeVisible();
      // The server returns: Retry resumes.
      game.down = false;
      await page.getByTestId("resume-retry").click();
      await runClock(page, 5_000);
      await expect(page.getByTestId("resume-fatal")).toHaveCount(0);
      await expect(page.getByTestId("lobby-connection-lost")).toHaveCount(0);
      await expect(page.getByText("Pick the system to activate").first()).toBeVisible();
    });

    test(`${vp.id}: a game that is gone says so`, async ({ page }, testInfo) => {
      test.setTimeout(60_000);
      const game = await open(page);
      game.snapshotStatus = 404;
      game.lobbyStatus = 404;
      game.dropSocket();
      await sleepAndWake(page, 10 * MIN);
      if (after) {
        const panel = page.getByTestId("resume-fatal");
        await expect(panel).toBeVisible({ timeout: 15_000 });
        await expect(panel).toContainText("not found");
        await expect(panel).not.toContainText(/Failed to fetch|TypeError/i);
        await expect(page.getByTestId("resume-back")).toHaveAttribute("href", "/");
      } else await page.waitForTimeout(4_000);
      await shot(page, testInfo, `${prefix}-${vp.id}-5-game-gone`);
    });
  });
}
