import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The legendary menu: each ready ability with its planet and printed card text.
test("decision UIs: legendary menu", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Legendary planet abilities"),
  });
  await page.getByTestId("legendary-panel").waitFor();
  await shot(page, testInfo, "4-legendary-menu", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
