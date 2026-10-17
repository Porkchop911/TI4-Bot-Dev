import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

// Nightly 2026-10-14 #13 (run o2-186-0210): after the game ended in the action phase, the read-only
// turn bar still said "Waiting for ..." with every button "Not your turn" under the Game Over banner.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

test(`${prefix} game over in the action phase`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const me = playerWithHand({ strategy_cards: ["pok2diplomacy", "pok6warfare"], exhausted_strategy_cards: ["pok6warfare"] });
  const game = await openMockedGame(page, {
    players: [me, opponent],
    phase: "action",
    round: 7,
    view: { active_player: opponent.id },
    turnStatus: { kind: "waiting_for_decision", seat: opponent.id, phase: "action", round: 7, stage: "Waiting for player" },
  });
  await page.getByTestId("turn-status-banner").waitFor();
  // The read-only bar is there while the other seat plays.
  await expect(page.getByTestId("turn-action-bar")).toBeVisible();
  game.send({
    type: "game_over",
    protocol_version: game.snapshot.protocol_version,
    game_id: GAME_ID,
    game_version: game.snapshot.game_version + 1,
    winner: opponent.id,
    final_scores: { [me.id]: 7, [opponent.id]: 10 },
  });
  await expect(page.getByTestId("turn-status-banner")).toContainText("Game Over");
  if (prefix === "after") await expect(page.getByTestId("turn-action-bar")).toHaveCount(0);
  await shot(page, testInfo, `${prefix}-game-over-action-phase`);
});
