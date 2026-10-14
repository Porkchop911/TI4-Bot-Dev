import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPayment, pickOnMap } from "./payment";

// Pick on map: payable planets are ringed with their value, the rest are dimmed, and the payment
// bar shows nothing paid yet and why Pay is disabled.
test("payment: payable planets on the map", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await shot(page, testInfo, "5b-payment-map-highlighted");
});
