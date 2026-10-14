import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

// The viewer is named "You" already, so the card no longer reads "You (You)".
test("player sheet: own card", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  const own = page.locator('[data-testid="player-card"][data-is-self="true"]');
  await own.waitFor();
  await shot(page, testInfo, "4-player-sheet", { of: own, pad: 12 });
});
