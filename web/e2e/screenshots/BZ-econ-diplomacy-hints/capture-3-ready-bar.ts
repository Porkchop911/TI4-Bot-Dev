import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReadyBar, prefix, SIZES, type SizeName } from "./scene";

// The ready-planets bar after the question is accepted: chips list the planets, lines show the cards.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`ready bar ${size}`, async ({ page }, testInfo) => {
    await openReadyBar(page, size, prefix !== "before");
    if (prefix !== "before") {
      await expect(page.getByTestId("ready-hint-leadership")).toContainText("yes");
      await expect(page.getByTestId("ready-hints-planets")).toHaveCount(0);
    }
    await shot(page, testInfo, `${prefix}-bar-${size}`);
  });
}
