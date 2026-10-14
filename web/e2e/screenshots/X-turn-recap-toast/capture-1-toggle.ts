import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame } from "./recap";

// The Recap button next to Sound and Toasts in the player sheet header: off by default, remembered
// in this browser once switched on.
test("recap: the toggle, off and on", async ({ page }, testInfo) => {
  await openCornerGame(page, { players: [playerWithHand(), opponent] });
  const header = page.getByTestId("player-sheet-panel").locator("h2").locator("..");
  const button = page.getByTestId("turn-recap-btn");
  await expect(button).toHaveAttribute("aria-pressed", "false");
  await shot(page, testInfo, "1a-toggle-off", { of: header, pad: 40 });
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await shot(page, testInfo, "1b-toggle-on", { of: header, pad: 40 });
});
