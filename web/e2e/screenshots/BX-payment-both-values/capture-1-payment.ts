import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment, pickOnMap } from "../D-planet-selection/payment";

/** `before` is taken on the commit without the change. */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

// Paying 5 resources: every payable planet shows its resource AND influence value beside it, in the
// list and on the map badge.
test("payment list shows both values", async ({ page }, testInfo) => {
  await openPayment(page);
  if (prefix === "after") await expect(page.getByTestId("planet-card-exhaust|jord-values")).toBeVisible();
  await shot(page, testInfo, `${prefix}-list-desktop`, { of: page.getByTestId("payment-drawer"), pad: 8 });
});

test("map badges show both values", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await shot(page, testInfo, `${prefix}-map-desktop`);
});

test("payment list on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPayment(page);
  await shot(page, testInfo, `${prefix}-list-phone`);
});
