import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment, pickOnMap } from "./payment";

// The map and the list share one selection: planets clicked on the map are ticked in the list.
test("payment: map picks appear in the list", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await page.getByTestId("planet-jord").click();
  await page.getByTestId("resume-decision-btn").click();
  await page.getByTestId("payment-drawer").waitFor();
  await shot(page, testInfo, "5e-payment-synced-list");
});
