import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, blockServer, foldDecision, newGame } from "./offlineSaves";

// The browser gives the page no IndexedDB (a private window in some browsers, blocked site data):
// the saves fall back to memory and a banner says so, on the list and above the game.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`storage warning ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    await page.setViewportSize(viewport);
    await blockServer(page);
    await page.addInitScript(() => {
      Object.defineProperty(window, "indexedDB", { value: undefined, configurable: true });
    });
    await page.goto("/offline");
    await expect(page.getByTestId("offline-storage-warning")).toBeVisible();
    await shot(page, testInfo, `1-warning-on-the-list-${size}`);

    await newGame(page, { players: 3, seat: 2, seed: 5 });
    await expect(page.getByTestId("offline-storage-warning")).toBeVisible();
    await foldDecision(page);
    await shot(page, testInfo, `2-warning-above-the-game-${size}`);
  });
}
