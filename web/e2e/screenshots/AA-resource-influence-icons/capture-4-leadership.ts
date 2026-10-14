import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// Leadership primary: the price per token, influence available and the payment planets use the influence icon.
test("Leadership panel: purchase staged", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Leadership: gain and buy command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  await page.getByTestId("token-buy-plus").click();
  await page.getByTestId("token-buy-plus").click();
  await expect(page.getByTestId("token-buy-count")).toHaveText("2");
  await shot(page, testInfo, "4a-leadership-staged", { of: panel, pad: 8 });
});

test("Leadership panel: change payment", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Leadership: change which planets pay");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  await page.getByTestId("token-buy-plus").click();
  await page.getByTestId("token-payment-change").click();
  await shot(page, testInfo, "4b-leadership-change-payment", { of: panel, pad: 8 });
});

test("Leadership panel on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openGalleryCase(page, "Leadership: change which planets pay");
  await page.getByTestId("command-token-panel").waitFor();
  await page.getByTestId("token-buy-plus").click();
  await page.getByTestId("token-payment-change").click();
  await shot(page, testInfo, "4c-leadership-phone");
});
