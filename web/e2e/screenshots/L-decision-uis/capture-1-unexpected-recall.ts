import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Unexpected Action recalls one of your command tokens: the system picker, not a list of ids.
test("decision UIs: Unexpected Action recall", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Unexpected Action: recall your token"),
  });
  await page.getByTestId("pending-choice-dialog").waitFor();
  await shot(page, testInfo, "1-unexpected-recall", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
