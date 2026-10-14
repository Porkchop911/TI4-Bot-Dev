import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

test("tooltip: trade goods", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand({ trade_goods: 4 }), opponent] });
  await hoverShot(page, testInfo, "03-trade-goods-tooltip", page.getByTestId("player-trade-goods").first());
});
