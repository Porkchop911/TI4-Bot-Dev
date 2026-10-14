import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// Hovering a hex lists its planets, anomalies, wormholes, units and command tokens.
test("tooltip: board tile hover info", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await hoverShot(page, testInfo, "01-board-tile", page.getByTestId("system-hex-18"), {
    tooltip: page.getByTestId("system-tooltip"),
  });
});
