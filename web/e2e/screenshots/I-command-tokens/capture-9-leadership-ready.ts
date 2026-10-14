import { test, expect } from "@playwright/test";
import { openGalleryCase } from "../_shared/gallery";
import { shot } from "../_shared/shot";

// Leadership primary: two tokens bought and all five assigned.
test("command tokens: Leadership: ready to confirm", async ({ page }, testInfo) => {
  await openGalleryCase(page, "Leadership: gain and buy command tokens");
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  const click = async (id: string, times = 1) => {
    for (let i = 0; i < times; i++) await page.getByTestId(id).click();
  };
  await click("token-buy-plus", 2);
  await click("token-plus-tactic", 2);
  await click("token-plus-fleet", 2);
  await click("token-plus-strategic");
  await expect(page.getByTestId("token-remaining")).toContainText("0");
  await expect(page.getByTestId("token-confirm")).toBeEnabled();
  await shot(page, testInfo, "9-leadership-ready", { of: panel, pad: 8 });
});
