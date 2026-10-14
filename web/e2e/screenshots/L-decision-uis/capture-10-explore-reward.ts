import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// An exploration card's reward choice: the card as printed above the options.
test("decision UIs: exploration reward", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Exploration card: choose the reward"),
  });
  await page.getByTestId("explore-card").waitFor();
  await shot(page, testInfo, "10-explore-reward", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
