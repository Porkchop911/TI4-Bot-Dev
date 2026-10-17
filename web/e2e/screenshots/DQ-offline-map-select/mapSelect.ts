import { expect, type Page } from "@playwright/test";
import { blockServer } from "../DP-offline-saves/offlineSaves";
import { SIZES } from "../DO-offline-game/offlineGame";

export { SIZES, blockServer };

/** Opens the real setup form (real engine in a Web Worker) with a nickname and a table size. */
export async function openForm(page: Page, players: number) {
  await blockServer(page);
  await page.goto("/offline");
  await page.getByTestId("offline-nickname").fill("Ada");
  await page.getByTestId("offline-player-count").selectOption(String(players));
}

/** Opens the picker and waits for the engine's layouts and the first picture. */
export async function openPicker(page: Page) {
  await page.getByTestId("offline-map-button").click();
  await expect(page.locator('[data-testid^="map-card-"]').nth(1)).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId("map-preview-board")).toBeVisible({ timeout: 30_000 });
  // The cards' own pictures arrive one by one.
  await expect(page.locator(".map-card__placeholder")).toHaveCount(0, { timeout: 30_000 });
}

/** Layout cards of the picker (random excluded), with whether each is the selected one. */
export async function layoutCards(page: Page) {
  return page
    .locator('[data-testid^="map-card-"]:not([data-testid="map-card-random"])')
    .evaluateAll((els) =>
      els.map((el) => ({ id: el.getAttribute("data-testid")!.replace("map-card-", ""), on: el.getAttribute("aria-pressed") === "true" })),
    );
}
