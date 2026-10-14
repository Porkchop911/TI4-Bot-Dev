import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Manipulate Investments: the card grid with the trade goods already on each card and the progress.
test("decision UIs: Manipulate Investments", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    table: {
      strategy_card_goods: { pok1leadership: 1, pok3politics: 2, pok5trade: 1 },
    },
    choice: galleryDecision("Manipulate Investments: place a trade good"),
  });
  await page.getByTestId("investments-panel").waitFor();
  await page.getByTestId("choice-option").nth(2).click();
  await shot(page, testInfo, "5-investments-pick");
});
