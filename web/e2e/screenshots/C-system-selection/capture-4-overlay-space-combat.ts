import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// The Space Combat overlay shows where fleets would meet before committing to an activation.
test("system selection: space combat overlay", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
  });
  await page.getByRole("button", { name: /Space Combat/ }).click();
  await page.getByTestId("system-activation-bar").waitFor();
  await shot(page, testInfo, "4-overlay-space-combat");
});
