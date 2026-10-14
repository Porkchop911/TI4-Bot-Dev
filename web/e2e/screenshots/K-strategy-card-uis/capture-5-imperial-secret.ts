import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Imperial without Mecatol Rex: the draw-a-secret outcome card with the hand.
test("strategy cards: Imperial draws a secret", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Imperial: draw a secret"),
  });
  await page.getByTestId("imperial-outcome").waitFor();
  await shot(page, testInfo, "5-imperial-secret", { of: page.getByTestId("objectives-modal"), pad: 8 });
});
