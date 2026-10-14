import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { completedCombatBoard, galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// While the player decides, each fleet card shows its supply and transport usage; the title explains the numbers (Non-fighter ships).
test("tooltip: 21-fleet-supply", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: completedCombatBoard({ phase: "resolving_hits" }),
    choice: galleryDecision("combat casualty"),
  });
  const modal = page.getByTestId("combat-resolution-modal");
  await modal.waitFor();
  await hoverShot(page, testInfo, "21-fleet-supply", modal.locator("[title^='Non-fighter ships']").first(), { native: true });
});
