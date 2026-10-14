import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// The per-card switch explains what a click does (also shown in artifact A).
test("tooltip: reaction mode switch", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "23-reaction-toggle", page.getByTestId("action-card-item-sabo1").locator("button").last(), { native: true });
});
