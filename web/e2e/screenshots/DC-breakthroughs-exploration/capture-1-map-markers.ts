import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { board, players } from "./_fixtures";

// Exploration on the map: an attachment (+), explored systems (count) and a waiting frontier token (?).
for (const [size, viewport] of Object.entries({ desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } })) {
  test(`map markers ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    const { list, state } = players();
    await openMockedGame(page, { board, players: list, state });
    await expect(page.getByTestId("exploration-mark-attachments-26")).toBeVisible();
    await expect(page.getByTestId("exploration-mark-explored-26")).toBeVisible();
    await expect(page.getByTestId("exploration-mark-explored-24")).toBeVisible();
    await expect(page.getByTestId("exploration-mark-explored-19")).toBeVisible();
    await expect(page.getByTestId("exploration-mark-explored-39")).toBeVisible();
    await expect(page.getByTestId("exploration-mark-frontier-41")).toBeVisible();
    await shot(page, testInfo, `map-${size}`);
  });
}
