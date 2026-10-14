import { test } from "@playwright/test";
import { openPayment } from "../D-planet-selection/payment";
import { VIEWPORT, shotAndCheck } from "./_layer";

// The payment list is the generic choice workflow modal (decision-modal around a choice-workflow panel).
test("dialog header layer: choice workflow modal", async ({ page }, testInfo) => {
  await page.setViewportSize(VIEWPORT);
  await openPayment(page);
  await shotAndCheck(page, testInfo, "3-workflow", page.locator(".decision-modal__panel"));
});
