import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// redistribute: the arrangement held now, nothing changed.
test("command tokens: redistribute: the arrangement held now, nothing changed", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Redistribute command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await expect(page.getByTestId("token-reset")).toBeDisabled();
  await shot(page, testInfo, "1-redistribute-initial", { of: panel, pad: 8 });
});
