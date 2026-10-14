import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";

// Trade primary: candidate seats with commodities now and after a replenish.
test("strategy cards: Trade replenish", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    // A third seat, so the table lists two candidates.
    players: [playerWithHand(), opponent, withPlayer(opponent, { id: "third_seat", faction: "sol" })],
    choice: galleryDecision("Trade: replenish another player"),
  });
  const panel = page.getByTestId("trade-replenish-panel");
  await panel.waitFor();
  await shot(page, testInfo, "3-trade-replenish", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
