import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, playToActionPhase, startGame } from "./offlineGame";

// A game played through the real UI into the action phase: the turn bar, the player sheet, the
// event log with its history controls (undo yes, turn redo no).
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`action phase ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(180_000);
    await page.setViewportSize(viewport);
    const id = await startGame(page);
    await playToActionPhase(page, id);
    await expect(page.getByTestId("offline-banner")).toBeVisible();
    await shot(page, testInfo, `3-action-phase-${size}`);
    // What is hidden offline: turn redo (event log).
    // Phones open the log from the bottom bar; the desktop has a toggle on the log itself.
    if (size === "desktop") await page.getByTestId("event-log-toggle").click();
    else await page.locator('button:text-is("Events")').click();
    await expect(page.getByTestId("event-log-list")).toBeVisible();
    await expect(page.getByTestId("turn-redo-btn")).toHaveCount(0);
    await expect(page.getByTestId("event-log-turn-redo")).toHaveCount(0);
    await shot(page, testInfo, `4-event-log-no-turn-redo-${size}`);
  });
}
