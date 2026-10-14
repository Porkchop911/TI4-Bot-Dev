import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Production capacity: the badge's native title text.
test("tooltip: Production capacity", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "09-production-capacity", page.getByTestId("player-production-capacity").first(), { native: true });
});
