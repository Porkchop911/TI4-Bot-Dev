import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";

// Prophecy of Kings picked: the line under the selector describes what changes.
test("strategy card set: create form, Prophecy of Kings selected", async ({ page }, testInfo) => {
  await page.goto("/");
  await page.getByLabel("Strategy cards").selectOption("pok");
  await expect(page.getByTestId("strategy-card-set-description")).toContainText("Prophecy of Kings");
  await shot(page, testInfo, "2-create-pok", { of: page.locator(".lobby-panel"), pad: 24 });
});
