import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { i48sOffer, openScene, SIZES, type SizeName } from "./scene";

// The question is asked as before (setting on by default) and now carries the switch.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`I48S offer with the switch ${size}`, async ({ page }, testInfo) => {
    const { advance, submissions } = await openScene(page, size);
    advance(1, null);
    advance(2, i48sOffer("i48s-1"));
    const switchRow = page.getByTestId("i48s-switch");
    await expect(switchRow).toBeVisible();
    await expect(page.getByTestId("i48s-switch-input")).not.toBeChecked();
    await page.waitForTimeout(1000);
    expect(submissions).toHaveLength(0);
    await shot(page, testInfo, `1-offer-${size}`);
  });
}
