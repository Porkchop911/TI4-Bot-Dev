import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// The map zoom buttons say what they do (Zoom Out shown; the Zoom In tooltip is centred on a button at the left edge of the window and is partly cut off).
test("tooltip: zoom control", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "02-zoom-controls", page.getByTitle("Zoom Out").first());
});
