import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// In a combat, the hit count beside each ship type opens the individual rolls on hover.
test("tooltip: combat roll results per ship type", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: completedCombatBoard({ phase: "resolving_hits" }),
  });
  await page.getByTestId("combat-resolution-modal").waitFor();
  const badge = page.getByTestId("combat-roll-group-gallery_seat-dreadnought");
  await hoverShot(page, testInfo, "16-roll-badge", badge, { pad: 60 });
});
