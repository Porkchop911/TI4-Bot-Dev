import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";
import { tradeChoice } from "./tradeChoice";

// Hacan across the table: buying and swapping its notes sit on the Promissory Notes tab of Quick deals.
test("hacan trade: promissory tab lists the ask shapes", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), withPlayer(opponent, { id: "other_seat", faction: "hacan", trade_goods: 3, commodities: 6 })],
    choice: tradeChoice(),
  });
  await page.getByTestId("trade-desk-modal").waitFor();
  await page.getByTestId("quick-deals").locator("summary").click();
  await page.getByTestId("trade-tab-promissory").click();
  await shot(page, testInfo, "1-promissory-tab", { of: page.getByTestId("trade-desk-modal"), pad: 8 });
});
