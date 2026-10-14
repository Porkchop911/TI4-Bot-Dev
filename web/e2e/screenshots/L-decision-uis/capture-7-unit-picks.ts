import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Scuttle (and Refit Troops): each unit named, placed, with what the card does to it.
test("decision UIs: unit picks", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Scuttle: which ship to scuttle"),
  });
  await page.getByTestId("unit-pick-panel").waitFor();
  await shot(page, testInfo, "7-unit-picks", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
