import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { acceptingScript, openScene } from "../AU-leadership-exact-payment/scene";
import { prefix } from "./scene";

// Preparing the Leadership secondary while another seat resolves the card: the payment chosen on the
// map is part of the plan. Before: tokens first (Buy +), then the payment. After: planets first.
test("leadership prep: payment on the map", async ({ page }, testInfo) => {
  await openScene(page, acceptingScript);
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("command-token-panel").waitFor();
  if (prefix === "before") {
    await page.getByTestId("token-buy-plus").click();
    await page.getByTestId("token-plus-fleet").click();
    await page.getByTestId("token-pay-on-map").click();
    await page.getByTestId("token-payment-bar").waitFor();
    for (const planet of ["lodor", "jord", "quann"]) await page.getByTestId(`planet-${planet}`).click();
  } else {
    await page.getByTestId("token-pay-on-map").click();
    await page.getByTestId("token-payment-bar").waitFor();
    for (const planet of ["jord", "quann"]) await page.getByTestId(`planet-${planet}`).click();
    await page.getByTestId("token-bar-pool-fleet").click();
  }
  await shot(page, testInfo, `${prefix}-8-prepare-map-payment`);
});
