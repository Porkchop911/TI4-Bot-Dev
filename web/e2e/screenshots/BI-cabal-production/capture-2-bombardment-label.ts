import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { bombardmentTarget, prefix } from "./scene";

test(`${prefix} bombardment target options name the player (already mapped before this change)`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: bombardmentTarget });
  await page.waitForTimeout(500);
  await shot(page, testInfo, `${prefix}-bombardment-target`);
  if (prefix === "before") return;
  await expect(page.getByText(/player_[0-9a-f]{6}/)).toHaveCount(0);
});
