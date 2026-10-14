import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Construction's second placement: step 2 of 2, PDS only.
test("strategy cards: Construction, second structure", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Construction: second structure"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("structure-step").waitFor();
  await page.getByTestId("system-hex-18").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await shot(page, testInfo, "2-construction-second");
});
