import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Controlled planets: the badge's native title text.
test("tooltip: Controlled planets", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "08-controlled-planets", page.getByTestId("player-controlled-planets").first(), { native: true });
});
