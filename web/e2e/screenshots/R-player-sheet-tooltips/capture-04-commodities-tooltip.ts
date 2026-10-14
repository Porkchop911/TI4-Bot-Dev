import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

test("tooltip: commodities", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand({ commodities: 2 }), opponent] });
  await hoverShot(page, testInfo, "04-commodities-tooltip", page.getByTestId("player-commodities").first());
});
