import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// gain: two of three assigned.
test("command tokens: gain: two of three assigned", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Gain command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await click("token-plus-tactic");
  await click("token-plus-strategic");
  await expect(page.getByTestId("token-remaining")).toContainText("1");
  await shot(page, testInfo, "6-gain-partial", { of: panel, pad: 8 });
});
