import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openDevice, prefix, SIZES, type SizeName } from "./scene";

// The Enigmatic Device's "which technology" question. Before: a plain option list. After: the
// normal technology research dialog, where only technologies whose prerequisites are met are
// pickable and Confirm submits the pick.
test.describe.configure({ retries: 2 });

for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`enigmatic device ${size}`, async ({ page }, testInfo) => {
    await openDevice(page, size);
    if (prefix === "before") {
      await shot(page, testInfo, `${prefix}-device-${size}`);
      return;
    }
    await page.getByTestId("technology-modal").waitFor();
    await expect(page.getByTestId("tech-card-amd")).toHaveAttribute("data-selectable", "true");
    await expect(page.getByTestId("tech-card-gd")).toHaveAttribute("data-selectable", "false");
    await expect(page.getByTestId("confirm-research-btn")).toBeDisabled();
    await page.getByTestId("tech-card-amd").click();
    await expect(page.getByTestId("confirm-research-btn")).toBeEnabled();
    await shot(page, testInfo, `${prefix}-device-${size}`);
  });
}
