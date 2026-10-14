import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Action cards a player holds are listed in the combat window; hovering one shows its effect.
test("tooltip: action card chip in combat", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: completedCombatBoard({ phase: "resolving_hits" }),
  });
  const modal = page.getByTestId("combat-resolution-modal");
  await modal.waitFor();
  await hoverShot(page, testInfo, "20-combat-card", modal.locator("[title*='Direct Hit']").first(), { native: true });
});
