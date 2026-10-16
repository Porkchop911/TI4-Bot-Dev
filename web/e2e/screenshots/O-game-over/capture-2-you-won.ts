import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { me, opponent } from "../_shared/players";

// The server pushes game_over when the game ends. The open tab must switch to the game-over banner
// without a reload (the client used to ignore the message and stay on the last phase banner).
test("game over: the viewer won", async ({ page }, testInfo) => {
  const game = await openMockedGame(page, { players: [me, opponent], phase: "status", round: 5 });
  await page.getByTestId("turn-status-banner").waitFor();
  game.send({
    type: "game_over",
    protocol_version: game.snapshot.protocol_version,
    game_id: GAME_ID,
    game_version: game.snapshot.game_version + 1,
    winner: me.id,
    final_scores: { [me.id]: 10, [opponent.id]: 9 },
  });
  await expect(page.getByTestId("turn-status-banner")).toContainText("Game Over");
  await shot(page, testInfo, "2-you-won", { of: page.getByTestId("turn-status-bar"), pad: 6 });
});
