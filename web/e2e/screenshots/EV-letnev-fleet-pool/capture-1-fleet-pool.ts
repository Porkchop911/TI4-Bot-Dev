import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, lowerFleetTo, openRedistribution, type SizeName } from "./scene";

// Letnev lowers its fleet pool from 4 to 0. "before" = the options the engine offered before the
// fix (fleet >= 2); "after" = the options of rules version 4 (fleet >= 0).
test.describe.configure({ retries: 2 });

for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`before: fleet 1 is blocked ${size}`, async ({ page }, testInfo) => {
    await openRedistribution(page, size, 2);
    await lowerFleetTo(page, 1);
    await expect(page.getByTestId("token-confirm")).toBeDisabled();
    await expect(page.getByTestId("token-blocker")).toContainText("at least 2");
    await shot(page, testInfo, `before-${size}`);
  });

  test(`after: fleet 0 can be confirmed ${size}`, async ({ page }, testInfo) => {
    await openRedistribution(page, size, 0);
    await lowerFleetTo(page, 0);
    await expect(page.getByTestId("token-count-fleet")).toHaveText("0");
    await expect(page.getByTestId("token-blocker")).toHaveCount(0);
    await expect(page.getByTestId("token-confirm")).toBeEnabled();
    await shot(page, testInfo, `after-${size}`);
  });

  test(`letnev shows the Armada fleet supply ${size}`, async ({ page }, testInfo) => {
    await openRedistribution(page, size, 0, true);
    await lowerFleetTo(page, 1);
    const chip = page.getByTestId("token-armada");
    await expect(chip).toBeVisible();
    await expect(chip).toHaveText("+2 supply");
    await expect(page.getByTestId("token-confirm")).toBeEnabled();
    const { width } = SIZES[size];
    const box = (await chip.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(width);
    if (size === "desktop") await expect(page.getByTestId("player-armada").first()).toBeVisible();
    await shot(page, testInfo, `letnev-${size}`);
  });
}
