import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Influence badge: the badge's native title text.
test("tooltip: Influence badge", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "06-influence", page.getByTestId("player-influence").first(), { native: true });
});
