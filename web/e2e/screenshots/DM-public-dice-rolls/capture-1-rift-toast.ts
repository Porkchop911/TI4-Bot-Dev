import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { openCornerGame, cornerCrop, pushEntry } from "../G-corner-notifications/corner";
import { rollEntry, riftDestroyed, riftSurvives, combatRound } from "./dice";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// The feedback case: another player's ship leaves a gravity rift. Everyone is told what the die
// showed and what it did to the ship.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`dice toast: another player's gravity rift roll ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
    pushEntry(game, rollEntry(riftDestroyed(opponent.id)));
    const toast = page.locator('[data-testid="corner-toast"][data-toast-kind="notice"]');
    await toast.waitFor();
    await expect(page.getByTestId("toast-notice-tag")).toHaveText("Dice");
    await expect(toast).toContainText("rolled Gravity rift in #67");
    await expect(toast).toContainText("2 · ship destroyed");
    await page.waitForTimeout(700);
    const box = (await toast.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
    await shot(page, testInfo, `1-rift-destroyed-${size}`, size === "desktop" ? cornerCrop(page) : {});
  });
}

// The roller sees their own roll too, and one combat round is one toast, not one per die.
test("dice toast: own roll, and a combat round merged", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushEntry(game, rollEntry(riftSurvives(actor)));
  const toast = page.locator('[data-testid="corner-toast"][data-toast-kind="notice"]');
  await toast.waitFor();
  await expect(toast).toContainText("8 · ship survives");
  await shot(page, testInfo, "2a-own-roll-survives", cornerCrop(page));
  await toast.click();
  await expect(toast).toHaveCount(0);
  pushEntry(game, rollEntry(combatRound(opponent.id, [8, 2], 1)));
  pushEntry(game, rollEntry(combatRound(opponent.id, [10, 6, 1], 1)));
  await toast.waitFor();
  await expect(toast).toHaveCount(1);
  await expect(toast).toContainText("8, 2, 10, 6, 1 · 2 hits");
  await page.waitForTimeout(700);
  await shot(page, testInfo, "2b-combat-round-merged", cornerCrop(page));
});
