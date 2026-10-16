import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openHeaderGame } from "./_header";

// And at 760 px: the buttons keep their size and the status text wraps instead.
test("header at 760", async ({ page }, testInfo) => {
  await openHeaderGame(page, 760);
  await shot(page, testInfo, "3-header-760", { of: page.locator(".game-header") });
});
