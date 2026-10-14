import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { me, opponent } from "../_shared/players";

// The status bar of a live game, before the end: the banner names the phase.
test("game over: the banner before the game ends", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [me, opponent], phase: "status", round: 5 });
  await page.getByTestId("turn-status-banner").waitFor();
  await shot(page, testInfo, "1-before", { of: page.getByTestId("turn-status-bar"), pad: 6 });
});
