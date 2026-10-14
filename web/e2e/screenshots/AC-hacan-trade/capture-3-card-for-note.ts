import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";
import { tradeChoice } from "./tradeChoice";

// Arbiters: an action card for one of Hacan's notes.
test("hacan trade: action card for a note", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), withPlayer(opponent, { id: "other_seat", faction: "hacan", trade_goods: 3, commodities: 6 })],
    choice: tradeChoice(),
  });
  await page.getByTestId("trade-desk-modal").waitFor();
  await page.getByTestId("quick-deals").locator("summary").click();
  await page.getByTestId("trade-tab-promissory").click();
  await page.getByTestId("trade-opt-cnrally>cf:hacan").click();
  await page.getByTestId("stage-status-valid").waitFor();
  await shot(page, testInfo, "3-card-for-note", { of: page.getByTestId("trade-desk-modal"), pad: 8 });
});
