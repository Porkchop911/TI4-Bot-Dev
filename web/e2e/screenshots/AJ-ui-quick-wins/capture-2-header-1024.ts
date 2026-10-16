import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openHeaderGame } from "./_header";

// The same header at 1024 px: the status bar wraps and Technologies and Objectives stay visible.
test("header at 1024", async ({ page }, testInfo) => {
  await openHeaderGame(page, 1024);
  await shot(page, testInfo, "2-header-1024", { of: page.locator(".game-header") });
});
