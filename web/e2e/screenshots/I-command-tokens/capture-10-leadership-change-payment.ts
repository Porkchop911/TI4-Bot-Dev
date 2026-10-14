import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// Leadership primary: one token bought, the payment changed from Auto-pay to Jord plus a trade good.
test("command tokens: Leadership: change payment", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Leadership: change which planets pay");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  await page.getByTestId("token-buy-plus").click();
  await page.getByTestId("token-payment-change").click();
  await page.getByTestId("token-payment-planet-lodor").click();
  await page.getByTestId("token-payment-planet-jord").click();
  await page.getByTestId("token-payment-goods-plus").click();
  await expect(page.getByTestId("token-payment-account")).toContainText("remainder 0");
  await shot(page, testInfo, "10-leadership-change-payment", { of: panel, pad: 8 });
});
