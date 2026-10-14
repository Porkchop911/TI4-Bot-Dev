import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// A ground force took a hit from an effect; its owner may sustain damage.
test("porkchop decisions: ground sustain", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Ground hit: sustain damage"),
  });
  await page.getByTestId("offer-card-panel").waitFor();
  await shot(page, testInfo, "1-ground-sustain", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
