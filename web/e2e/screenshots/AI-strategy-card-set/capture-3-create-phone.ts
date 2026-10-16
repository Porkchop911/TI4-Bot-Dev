import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";

// Phone width, 390 px: the selector and its description stack with the other fields.
test("strategy card set: create form on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await expect(page.getByLabel("Strategy cards")).toBeVisible();
  await shot(page, testInfo, "3-create-phone");
});
