import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { groundCombatBoard, outcomes } from "./fixture";

test("ground combat result: attacker wins", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: groundCombatBoard(outcomes.attackerWins) });
  await page.getByTestId("ground-combat-result-summary").waitFor();
  await shot(page, testInfo, "1-attacker-win");
});
