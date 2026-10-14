import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Several options share a planet: after picking it, one button per structure appears.
test("planet selection: planet then structure", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Place a structure"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("system-hex-18").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await shot(page, testInfo, "2-planet-then-structure");
});
