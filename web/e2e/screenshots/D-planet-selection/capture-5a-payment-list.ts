import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment } from "./payment";

// Paying: the list of ready planets with what each is worth, now with Auto-pay and Pick on map.
test("payment: the list", async ({ page }, testInfo) => {
  await openPayment(page);
  await shot(page, testInfo, "5a-payment-list");
});
