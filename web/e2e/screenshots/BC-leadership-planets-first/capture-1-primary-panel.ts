import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// Leadership primary on desktop: the panel as it opens (nothing staged).
test("leadership primary: panel as opened", async ({ page }, testInfo) => {
  await openChoice(page, primaryChoice());
  await shot(page, testInfo, `${prefix}-1-primary-panel`);
});
