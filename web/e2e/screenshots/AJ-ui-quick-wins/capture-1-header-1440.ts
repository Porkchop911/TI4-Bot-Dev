import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openHeaderGame } from "./_header";

// The header at full width: the status bar, the human stage label in the banner, and both buttons.
test("header at 1440", async ({ page }, testInfo) => {
  await openHeaderGame(page, 1440, 900);
  await shot(page, testInfo, "1-header-1440", { of: page.locator(".game-header") });
});
