import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { me, opponent } from "../_shared/players";

// Same push, an opponent as the winner.
test("game over: an opponent won", async ({ page }, testInfo) => {
  const game = await openMockedGame(page, { players: [me, opponent], phase: "status", round: 5 });
  await page.getByTestId("turn-status-banner").waitFor();
  game.send({
    type: "game_over",
    protocol_version: game.snapshot.protocol_version,
    game_id: GAME_ID,
    game_version: game.snapshot.game_version + 1,
    winner: opponent.id,
    final_scores: { [me.id]: 9, [opponent.id]: 10 },
  });
  await expect(page.getByTestId("turn-status-banner")).toContainText("Game Over");
  await shot(page, testInfo, "3-opponent-won", { of: page.getByTestId("turn-status-bar"), pad: 6 });
});
