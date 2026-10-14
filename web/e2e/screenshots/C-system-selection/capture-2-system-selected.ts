import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Clicking a highlighted hex opens the inspector and the confirm bar for that system.
test("system selection: a system is selected", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
  });
  await page.getByTestId("system-hex-26").click();
  await page.getByTestId("system-inspector").waitFor();
  await shot(page, testInfo, "2-system-selected");
});
