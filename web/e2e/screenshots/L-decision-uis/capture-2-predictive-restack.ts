import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Predictive Intelligence: plan the whole restack on the pools; each move is sent as its own answer.
test("decision UIs: Predictive Intelligence restack", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Predictive Intelligence: restack tokens"),
  });
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  for (let i = 0; i < 2; i++) {
    await page.getByTestId("token-minus-tactic").click();
    await page.getByTestId("token-plus-strategic").click();
  }
  await shot(page, testInfo, "2-predictive-restack", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
