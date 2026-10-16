import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { groundCombatBoard, outcomes } from "./fixture";

test.use({ viewport: { width: 390, height: 844 } });

test("ground combat result: phone width", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: groundCombatBoard(outcomes.attackerWins) });
  await page.getByTestId("ground-combat-result-summary").waitFor();
  await page.getByTestId("ground-combat-result-summary").scrollIntoViewIfNeeded();
  await shot(page, testInfo, "4-phone");
});
