import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";
import { tradeChoice } from "./tradeChoice";

// Paying commodities for Hacan's Trade Convoys: the summary names what is paid and what is received.
test("hacan trade: pay commodities for a note", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), withPlayer(opponent, { id: "other_seat", faction: "hacan", trade_goods: 3, commodities: 6 })],
    choice: tradeChoice(),
  });
  await page.getByTestId("trade-desk-modal").waitFor();
  await page.getByTestId("quick-deals").locator("summary").click();
  await page.getByTestId("trade-tab-promissory").click();
  await page.getByTestId("trade-opt-cpconvoys:hacan:3").click();
  await page.getByTestId("stage-status-valid").waitFor();
  await shot(page, testInfo, "2-pay-for-note", { of: page.getByTestId("trade-desk-modal"), pad: 8 });
});
