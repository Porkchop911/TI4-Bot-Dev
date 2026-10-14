import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";

// Spy (and every card that asks for a player): the printed card, and who each candidate is.
test("decision UIs: player picks", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [
      playerWithHand(),
      withPlayer(opponent, { id: "other_seat", victory_points: 5, trade_goods: 4, commodities: 2, action_cards_count: 3 }),
      withPlayer(opponent, { id: "third_seat", faction: "sol", victory_points: 2, trade_goods: 0, commodities: 1, action_cards_count: 1 }),
    ],
    choice: galleryDecision("Spy: rob which player"),
  });
  await page.getByTestId("pick-card").waitFor();
  await shot(page, testInfo, "3-player-picks", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});
