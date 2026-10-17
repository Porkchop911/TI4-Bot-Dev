import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, startGame } from "./offlineGame";

// The first decision of a real offline game: the engine runs in a Web Worker on this device and
// the page has no server (every /api, /ws and /health call is blocked).
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`first decision ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await startGame(page);
    await expect(page.getByTestId("pending-choice-dialog")).toBeVisible();
    await expect(page.getByTestId("submit-choice-button")).toBeVisible();
    await shot(page, testInfo, `1-strategy-choice-${size}`);
    // The decision dialog can be folded away to look at the board and the banner underneath.
    await page.getByRole("button", { name: /minimi[sz]e|collapse|^[−-]$/i }).first().click().catch(() => undefined);
    await expect(page.getByTestId("offline-banner")).toBeVisible();
    await shot(page, testInfo, `2-board-and-banner-${size}`);
  });
}
