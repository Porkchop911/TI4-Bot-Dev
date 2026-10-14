import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// redistribute: one token moved from tactic to strategy, ready to confirm.
test("command tokens: redistribute: one token moved from tactic to strategy, ready to confirm", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Redistribute command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await click("token-minus-tactic");
  await click("token-plus-strategic");
  await expect(page.getByTestId("token-confirm")).toBeEnabled();
  await shot(page, testInfo, "4-redistribute-valid", { of: panel, pad: 8 });
});
