import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openTurnMenu, prefix, SIZES, type SizeName } from "./scene";

// Economic Initiative in the Action cards menu: the cultural planets it readies, with their state.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`economic initiative ${size}`, async ({ page }, testInfo) => {
    await openTurnMenu(page, size, prefix !== "before");
    await page.getByTestId("turn-bar-cards").click();
    const row = page.getByTestId("turn-bar-item-action_card|0");
    await row.waitFor();
    if (prefix !== "before") {
      await expect(row).toContainText("readies 2 of 3");
      await row.hover();
      await expect(page.getByTestId("turn-bar-info-planet-lodor")).toContainText("exhausted, will ready");
      await expect(page.getByTestId("turn-bar-info-planet-quann")).toContainText("already ready");
    }
    await shot(page, testInfo, `${prefix}-economic-${size}`);
  });
}
