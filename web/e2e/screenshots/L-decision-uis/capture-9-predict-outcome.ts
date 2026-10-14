import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// A rider asks which outcome of the agenda to predict.
test("decision UIs: predict the outcome", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Imperial Rider: predict the outcome"),
  });
  await page.getByTestId("predict-outcome-panel").waitFor();
  await shot(page, testInfo, "9-predict-outcome", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
