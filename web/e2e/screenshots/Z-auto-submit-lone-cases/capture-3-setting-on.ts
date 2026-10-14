import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openLoneGame } from "./lone";

// The player sheet header: the new button next to the sound and toast toggles. On by default.
test("setting: auto lone moves on (default)", async ({ page }, testInfo) => {
  await openLoneGame(page);
  const button = page.getByTestId("auto-submit-btn");
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await expect(button).toContainText("Auto");
  await shot(page, testInfo, "3-setting-on", { of: page.getByTestId("player-sheet-panel").locator("h2").locator(".."), pad: 40 });
});
