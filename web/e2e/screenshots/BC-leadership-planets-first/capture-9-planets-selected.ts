import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// After only: the panel with planets picked first (Jord and Centauri, 5 influence), the tally and
// the tokens still to assign. Resource values show next to every planet's influence.
test("leadership primary: planets selected in the list", async ({ page }, testInfo) => {
  test.skip(prefix === "before", "the planet-first panel does not exist before");
  await openChoice(page, primaryChoice());
  await page.getByTestId("token-payment-planet-centauri").click();
  await page.getByTestId("token-payment-planet-jord").click();
  await shot(page, testInfo, `${prefix}-9-planets-selected`, { of: page.getByTestId("command-token-panel"), pad: 8 });
});
