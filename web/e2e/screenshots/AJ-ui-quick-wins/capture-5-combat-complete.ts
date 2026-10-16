import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// A finished combat says "Combat complete" once, and the title no longer repeats round and phase.
test("combat modal: finished battle", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: completedCombatBoard() });
  await page.getByTestId("combat-resolution-modal").waitFor();
  await shot(page, testInfo, "5-combat-complete");
});
