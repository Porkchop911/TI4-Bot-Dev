import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";

// The three sets the selector offers. A native dropdown is not rendered into screenshots when
// open, so the select is expanded into a list box showing every option label.
test("strategy card set: the offered sets", async ({ page }, testInfo) => {
  await page.goto("/");
  const select = page.getByLabel("Strategy cards");
  const labels = await select.locator("option").allTextContents();
  expect(labels).toEqual(["Thunder's Edge (default)", "Prophecy of Kings", "Base game + Codex I"]);
  await select.evaluate((element: HTMLSelectElement) => {
    element.size = element.options.length;
  });
  await shot(page, testInfo, "4-create-options", { of: page.locator(".lobby-panel"), pad: 24 });
});
