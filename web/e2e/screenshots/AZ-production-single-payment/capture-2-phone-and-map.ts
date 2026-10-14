import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { confirmBuilds, openProduction, phone, prefix, stageAndConfirm, waitForPanel } from "./scene";

// The first payment question at phone width (390 px), and the same panel minimised so the map is
// the control (the payment bar of the Leadership map payment, now for the whole production).
test(`${prefix} payment question, phone`, async ({ page }, testInfo) => {
  await page.setViewportSize(phone);
  await openProduction(page);
  await stageAndConfirm(page);
  await confirmBuilds(page);
  if (prefix === "before") {
    await page.getByTestId("payment-drawer").waitFor();
    await page.getByTestId("auto-pay-btn").click();
    await expect(page.getByTestId("confirm-payment-btn")).toBeEnabled();
  } else {
    await waitForPanel(page);
    await expect(page.getByTestId("payment-drawer-title")).toContainText("Pay for 3 units: total");
  }
  await shot(page, testInfo, `${prefix}-5-prompt-phone`);
  // The tally and the confirm button sit below the planet list on a phone.
  await page.getByTestId("confirm-payment-btn").scrollIntoViewIfNeeded();
  await shot(page, testInfo, `${prefix}-5b-prompt-phone-lower`);
});

test(`${prefix} payment on the map`, async ({ page }, testInfo) => {
  test.skip(prefix === "before", "the map bar for a whole production is new");
  await openProduction(page);
  await stageAndConfirm(page);
  await confirmBuilds(page);
  await waitForPanel(page);
  await page.getByTestId("close-payment-drawer").click();
  await page.getByTestId("payment-bar").waitFor();
  await expect(page.getByTestId("payment-bar-owed")).toContainText("Pay for 3 units: total");
  await shot(page, testInfo, `${prefix}-6-map-bar`);
});
