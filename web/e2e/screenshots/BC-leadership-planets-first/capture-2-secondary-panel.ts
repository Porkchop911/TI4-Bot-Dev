import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, secondaryChoice } from "./scene";

// Leadership secondary on desktop: the purchase question as it opens (nothing staged).
test("leadership secondary: panel as opened", async ({ page }, testInfo) => {
  await openChoice(page, secondaryChoice());
  await shot(page, testInfo, `${prefix}-2-secondary-panel`);
});
