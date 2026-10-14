import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Munitions Reserves: offered at the start of each combat round, before the dice are rolled.
test("decision UIs: Munitions Reserves", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 5 }), opponent],
    choice: galleryDecision("Munitions Reserves: reroll misses"),
  });
  await page.getByTestId("ability-offer-panel").waitFor();
  await shot(page, testInfo, "6-munitions-reroll", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
