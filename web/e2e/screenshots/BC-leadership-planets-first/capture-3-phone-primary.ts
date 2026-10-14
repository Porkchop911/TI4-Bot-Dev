import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// Leadership primary at phone width (390x844).
test("leadership primary: phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openChoice(page, primaryChoice());
  await shot(page, testInfo, `${prefix}-3-phone-primary`);
});
