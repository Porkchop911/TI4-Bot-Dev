import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, openTransit, prefix } from "./scene";

// Only on the commit before the change (TI4_SHOT_PREFIX=before): the planet selection bar that
// grouped every Transit Diodes move under its destination planet. The PNGs are committed in before/.
for (const size of SIZES) {
  test(`before-${size.label}`, async ({ browser, baseURL }, testInfo) => {
    test.skip(prefix !== "before", "before shots are taken on the base commit");
    const { context, page } = await openTransit(browser, size, baseURL);
    await page.getByTestId("planet-selection-bar").waitFor();
    await shot(page, testInfo, `before-1-initial-${size.label}`);
    await page.getByTestId("planet-chip-jord").click();
    await shot(page, testInfo, `before-2-destination-jord-${size.label}`);
    await page.getByTestId("planet-chip-sol_jord").click();
    await shot(page, testInfo, `before-3-destination-sol-jord-${size.label}`);
    await context.close();
  });
}
