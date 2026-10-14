import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// A held action card shows its name, phase and full text.
test("tooltip: action card in hand", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "11-action-card", page.getByTestId("action-card-item-direct_hit").locator("button").first(), { native: true });
});
