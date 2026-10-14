import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Imperial while holding Mecatol Rex: the +1 VP outcome card above the objectives.
test("strategy cards: Imperial with Mecatol", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Imperial: you hold Mecatol Rex"),
  });
  await page.getByTestId("imperial-outcome").waitFor();
  await shot(page, testInfo, "4-imperial-mecatol", { of: page.getByTestId("objectives-modal"), pad: 8 });
});
