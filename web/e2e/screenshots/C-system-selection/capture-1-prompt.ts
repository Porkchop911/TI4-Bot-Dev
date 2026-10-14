import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Tactical action, step one: every system the player may activate is highlighted on the map.
test("system selection: prompt", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
  });
  await page.getByTestId("system-activation-bar").waitFor();
  await shot(page, testInfo, "1-prompt");
});
