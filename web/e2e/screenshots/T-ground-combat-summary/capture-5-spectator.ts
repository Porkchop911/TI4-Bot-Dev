import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { groundCombatBoard, outcomes } from "./fixture";

// A spectator (no seat) gets the same closing card: it is built from the public invasion step.
test("ground combat result: spectator view", async ({ page }, testInfo) => {
  await openMockedGame(page, { spectator: true, players: [playerWithHand(), opponent], board: groundCombatBoard(outcomes.defenderHolds) });
  await page.getByTestId("ground-combat-result-summary").waitFor();
  await shot(page, testInfo, "5-spectator");
});
