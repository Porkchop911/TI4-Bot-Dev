import { expect, test } from "@playwright/test";
import { storedSetup } from "../../offlineDriver";
import { shot } from "../_shared/shot";
import { SIZES, layoutCards, openForm, openPicker } from "./mapSelect";
import { requireEngine } from "../DO-offline-game/offlineGame";

// A game started with a picked layout: the choice is in the save (setup.map) and the board of the
// real game is that layout. Seed fixed so the pictures repeat.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`game started with a picked map ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(180_000);
    requireEngine();
    await page.setViewportSize(viewport);
    await openForm(page, 4);
    await page.getByTestId("offline-seed").fill("4242");
    await openPicker(page);
    const other = (await layoutCards(page)).find((c) => !c.on && /Hyperlane/i.test(c.id)) ?? (await layoutCards(page)).find((c) => !c.on)!;
    await page.getByTestId(`map-card-${other.id}`).click();
    await page.getByTestId("map-picker-done").click();
    await page.getByTestId("offline-start").click();
    await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 60_000 });
    const id = decodeURIComponent(/\/offline\/games\/([^/?#]+)/.exec(page.url())![1]);
    expect((await storedSetup(page, id))?.map).toBe(other.id);
    await page.waitForTimeout(1500);
    await shot(page, testInfo, `7-game-board-${size}`);
  });
}
