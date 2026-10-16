import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openSecondary, prefix, SIZES, type SizeName } from "./scene";

// The Diplomacy secondary question: which planets could be readied and which spending cards were played.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`diplomacy secondary ${size}`, async ({ page }, testInfo) => {
    await openSecondary(page, size, prefix !== "before");
    if (prefix !== "before") {
      await expect(page.getByTestId("ready-hint-planet-lodor")).toBeVisible();
      await expect(page.getByTestId("ready-hint-leadership")).toContainText("yes");
      await expect(page.getByTestId("ready-hint-technology")).toContainText("still to come");
      await expect(page.getByTestId("ready-hint-warfare")).toContainText("not in play");
      const { width } = SIZES[size];
      const box = (await page.getByTestId("ready-hints").boundingBox())!;
      expect(box.x + box.width).toBeLessThanOrEqual(width);
    }
    await shot(page, testInfo, `${prefix}-secondary-${size}`);
  });
}
