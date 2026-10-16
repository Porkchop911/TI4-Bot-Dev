import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment, pickOnMap } from "../D-planet-selection/payment";

// Paying 5 resources: the bar says "Pay [icon] 5", the planets on the map carry their worth as icon + number.
test("payment bar and map marks", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await shot(page, testInfo, "2a-payment-bar-map");
  await shot(page, testInfo, "2b-payment-bar", { of: page.getByTestId("payment-bar"), pad: 8 });
});

// The same after Auto-pay staged planets (a staged mark is green with a check; the tally shows paid / owed).
test("payment bar with staged planets", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await page.getByTestId("auto-pay-btn").click();
  await shot(page, testInfo, "2c-payment-bar-staged", { of: page.getByTestId("payment-bar"), pad: 8 });
  await shot(page, testInfo, "2d-payment-map-staged");
});

// The full payment list: totals and each planet's worth.
test("payment drawer list", async ({ page }, testInfo) => {
  await openPayment(page);
  await page.getByTestId("planet-card-exhaust|jord").click();
  await shot(page, testInfo, "2e-payment-drawer", { of: page.getByTestId("payment-drawer"), pad: 8 });
});

test("payment bar on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPayment(page);
  await pickOnMap(page);
  await page.getByTestId("auto-pay-btn").click();
  await shot(page, testInfo, "2f-payment-bar-phone");
});
