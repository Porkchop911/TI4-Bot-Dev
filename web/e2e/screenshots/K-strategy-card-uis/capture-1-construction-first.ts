import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Construction's first placement: the bar names the step and offers a dock or a PDS.
test("strategy cards: Construction, first structure", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Place a structure"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("structure-step").waitFor();
  await page.getByTestId("system-hex-18").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await shot(page, testInfo, "1-construction-first");
});
