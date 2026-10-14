import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Hacan's commander: spend trade goods for two more votes each, after the planets.
test("porkchop decisions: vote trade goods", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 4 }), opponent],
    choice: galleryDecision("Hacan commander: spend trade goods for votes"),
  });
  await page.getByTestId("vote-goods-panel").waitFor();
  await page.getByTestId("vote-goods-more").click();
  await page.getByTestId("vote-goods-more").click();
  await shot(page, testInfo, "7-vote-goods", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
