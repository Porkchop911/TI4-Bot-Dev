import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAMES, SIZES, openOffline } from "./offline";

for (const [size, viewport] of Object.entries(SIZES)) {
  test(`banner and queued feedback ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openOffline(page, "/offline", { games: GAMES, replayMs: 60 });
    await page.getByTestId("offline-resume").nth(1).click();
    await expect(page.getByTestId("offline-game-placeholder")).toBeVisible();
    await expect(page.getByTestId("offline-missing")).toContainText("Turn redo");
    await expect(page.getByTestId("offline-missing")).toContainText("Advisor");
    await shot(page, testInfo, `9-banner-${size}`);

    // The game screen (T5) marks the offline game active; here the capture plays that part.
    await page.waitForFunction(() => (window as unknown as Record<string, unknown>).__TI4_OFFLINE_DEV__);
    await page.evaluate(() => {
      const dev = (window as unknown as { __TI4_OFFLINE_DEV__: { setActiveOfflineGame: (g: unknown) => void } }).__TI4_OFFLINE_DEV__;
      dev.setActiveOfflineGame({ gameId: "off_b", local: () => ({ seed: 7, setup: {}, answers: ["a", "b"], engine: { build: "0.1.0", content_digest: "d" }, wasm_sha256: "ab".repeat(32), save: { seed: 7, setup: {}, answers: ["a", "b"] } }) });
    });
    await page.getByTestId("feedback-button").click();
    await page.getByTestId("feedback-text").fill("The bot took the wrong planet.");
    await shot(page, testInfo, `10-feedback-sheet-${size}`);
    await page.getByTestId("feedback-submit").click();
    await expect(page.getByTestId("feedback-queued")).toHaveText("Will be sent");
    await shot(page, testInfo, `11-feedback-queued-${size}`);
    await page.getByTestId("feedback-done").click();
    await expect(page.getByTestId("offline-feedback-chip")).toHaveText("1 feedback report will be sent");
    await shot(page, testInfo, `12-queue-chip-${size}`);
  });
}
