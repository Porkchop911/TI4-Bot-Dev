import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// Desktop: the panel minimised so the map pays. Before: buy a token first, then pay. After: start on
// the map, click Dal Bootha and Rarron (no resources); the bar shows 4 influence buys 1 token.
test("leadership primary: pay on the map", async ({ page }, testInfo) => {
  await openChoice(page, primaryChoice());
  if (prefix === "before") {
    await page.getByTestId("token-buy-plus").click();
    await page.getByTestId("token-pay-on-map").click();
    await page.getByTestId("token-payment-bar").waitFor();
  } else {
    await page.getByTestId("token-pay-on-map").click();
    await page.getByTestId("token-payment-bar").waitFor();
    await page.getByTestId("planet-dal_bootha").click();
    await page.getByTestId("planet-rarron").click();
    await page.getByTestId("token-bar-details").click();
  }
  await shot(page, testInfo, `${prefix}-5-pay-on-map`);
});
