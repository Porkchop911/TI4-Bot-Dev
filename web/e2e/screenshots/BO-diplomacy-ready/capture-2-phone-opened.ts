import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReady, prefix } from "./scene";

// 390 x 844: the same question on a phone; the map stays visible above the bar.
test("diplomacy ready: phone, as opened", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openReady(page);
  await shot(page, testInfo, `${prefix}-2-phone-opened`);
});
