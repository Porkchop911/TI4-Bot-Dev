import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// A card asks for a planet (Mining Initiative). The bar names the card; candidates glow on the map.
test("planet selection: card pick, before and after choosing on the map", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("planet selection"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await shot(page, testInfo, "1a-card-pick-prompt");
  await page.getByTestId("system-hex-26").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await shot(page, testInfo, "1b-card-pick-chosen");
});
