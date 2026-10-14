import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Controlled systems: the badge's native title text.
test("tooltip: Controlled systems", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "07-controlled-systems", page.getByTestId("player-controlled-systems").first(), { native: true });
});
