import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Resources badge: ready out of total: the badge's native title text.
test("tooltip: Resources badge: ready out of total", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "05-resources", page.getByTestId("player-resources").first(), { native: true });
});
