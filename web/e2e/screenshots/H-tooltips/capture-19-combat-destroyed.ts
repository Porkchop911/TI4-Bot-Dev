import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The red minus on a ship row counts the ships destroyed in this step.
test("tooltip: ships destroyed this step", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: completedCombatBoard({ phase: "resolving_hits" }),
  });
  await page.getByTestId("combat-resolution-modal").waitFor();
  await hoverShot(page, testInfo, "19-combat-destroyed", page.locator('[data-testid^="combat-destroyed-"]').first(), { native: true });
});
