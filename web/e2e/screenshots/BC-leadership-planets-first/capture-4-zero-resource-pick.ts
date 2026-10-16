import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// One token bought with the suggested payment. Centauri (3 influence, 1 resource) pays exactly;
// Dal Bootha + Rarron (2 + 2, no resources) waste 1 influence but exhaust nothing that produces.
test("leadership primary: suggested payment for one token", async ({ page }, testInfo) => {
  await openChoice(page, primaryChoice());
  await page.getByTestId("token-buy-plus").click();
  await shot(page, testInfo, `${prefix}-4-suggested-payment`, { of: page.getByTestId("command-token-panel"), pad: 8 });
});
