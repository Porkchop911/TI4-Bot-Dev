import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Bio-Stims: ready a planet on the map, or an exhausted technology from the chips in the bar.
test("planet selection: planet or technology", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Bio-Stims"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await shot(page, testInfo, "3-planet-or-technology");
});
