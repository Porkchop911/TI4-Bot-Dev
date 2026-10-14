import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The resource/influence overlay prints each planet's resources and influence on the map while choosing.
test("system selection: resources and influence overlay", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
  });
  await page.getByTestId("overlay-btn-economy").click();
  await page.getByTestId("system-activation-bar").waitFor();
  await shot(page, testInfo, "3-overlay-resources");
});
