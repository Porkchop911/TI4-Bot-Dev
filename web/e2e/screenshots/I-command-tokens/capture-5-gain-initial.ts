import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// gain: three tokens to assign.
test("command tokens: gain: three tokens to assign", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Gain command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await expect(page.getByTestId("token-confirm")).toBeDisabled();
  await shot(page, testInfo, "5-gain-initial", { of: panel, pad: 8 });
});
