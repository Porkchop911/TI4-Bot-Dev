import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// A held secret objective shows its phase, points and requirement.
test("tooltip: secret objective", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "12-secret-objective", page.getByTestId("secret-objective-item-faa").locator("button").first(), { native: true });
});
