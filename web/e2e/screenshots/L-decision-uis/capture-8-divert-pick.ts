import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Divert Funding: each technology as printed, and which half of the card this is.
test("decision UIs: technology picks", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Divert Funding: which technology to return"),
  });
  await page.getByTestId("technology-pick-panel").waitFor();
  await shot(page, testInfo, "8-divert-pick", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
