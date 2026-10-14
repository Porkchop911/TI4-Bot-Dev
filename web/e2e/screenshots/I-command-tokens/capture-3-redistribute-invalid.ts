import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// redistribute: all tokens placed but fleet below the minimum.
test("command tokens: redistribute: all tokens placed but fleet below the minimum", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Redistribute command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await click("token-minus-fleet", 3);
  await click("token-plus-tactic", 2);
  await click("token-plus-strategic");
  await expect(page.getByTestId("token-blocker")).toBeVisible();
  await expect(page.getByTestId("token-confirm")).toBeDisabled();
  await shot(page, testInfo, "3-redistribute-invalid", { of: panel, pad: 8 });
});
