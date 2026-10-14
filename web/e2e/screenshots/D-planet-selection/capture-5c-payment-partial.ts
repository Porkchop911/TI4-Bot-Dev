import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment, pickOnMap } from "./payment";

// A click on a payable planet stages it (green ring and tick). 4 of 5 resources are paid, 1 is
// remaining, and the bar says so.
test("payment: partially paid from the map", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await page.getByTestId("planet-jord").click();
  await page.getByTestId("payment-bar-remaining").getByText("1 remaining").waitFor();
  await shot(page, testInfo, "5c-payment-partial");
});
