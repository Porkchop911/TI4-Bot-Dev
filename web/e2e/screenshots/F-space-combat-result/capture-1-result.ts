import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The finished space combat: both fleets with what each side rolled, hit and lost, and the verdict.
test("space combat: result of a finished battle", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: completedCombatBoard() });
  await page.getByTestId("combat-resolution-modal").waitFor();
  await shot(page, testInfo, "1-result");
});
