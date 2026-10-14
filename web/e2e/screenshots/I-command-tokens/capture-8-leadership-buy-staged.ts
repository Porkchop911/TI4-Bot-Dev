import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// Leadership primary: two tokens bought, two of the five assigned.
test("command tokens: Leadership: purchase staged", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Leadership: gain and buy command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await click("token-buy-plus", 2);
  await click("token-plus-tactic");
  await click("token-plus-strategic");
  await expect(page.getByTestId("token-total")).toContainText("5");
  await expect(page.getByTestId("token-remaining")).toContainText("3");
  await shot(page, testInfo, "8-leadership-purchase-staged", { of: panel, pad: 8 });
});
