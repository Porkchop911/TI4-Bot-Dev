import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// redistribute: tokens moved out of tactic, one not yet placed.
test("command tokens: redistribute: tokens moved out of tactic, one not yet placed", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Redistribute command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await click("token-minus-tactic", 2);
  await click("token-plus-strategic");
  await expect(page.getByTestId("token-remaining")).toContainText("1");
  await shot(page, testInfo, "2-redistribute-moving", { of: panel, pad: 8 });
});
