import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";

// The create-game form: Strategy cards is preselected to Thunder's Edge (default).
test("strategy card set: create form, default Thunder's Edge", async ({ page }, testInfo) => {
  await page.goto("/");
  await expect(page.getByTestId("lobby-container")).toBeVisible();
  await expect(page.getByLabel("Strategy cards")).toHaveValue("te");
  await expect(page.getByTestId("strategy-card-set-description")).toContainText("Thunder's Edge");
  await shot(page, testInfo, "1-create-default", { of: page.locator(".lobby-panel"), pad: 24 });
});
