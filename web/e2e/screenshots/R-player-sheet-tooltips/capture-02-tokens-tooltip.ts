import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

test("tooltip: command token line names each number", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ tactic_tokens: 3, fleet_tokens: 3, strategic_tokens: 2 }), opponent],
  });
  await hoverShot(page, testInfo, "02-tokens-tooltip", page.getByTestId("player-command-tokens").first());
});
