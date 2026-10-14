import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment, pickOnMap } from "./payment";

// Auto-pay stages the planets that cover the bill with the least waste (Jord 4 + Quann 1 = 5),
// without paying: the bar is ready and Pay is enabled.
test("payment: auto-pay suggestion", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await page.getByTestId("auto-pay-btn").click();
  await page.getByTestId("payment-bar-remaining").getByText("Covered").waitFor();
  await shot(page, testInfo, "5d-payment-auto-pay");
});
