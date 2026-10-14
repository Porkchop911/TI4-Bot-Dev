import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { acceptingScript, openScene, prefix, prepareAndSave } from "./scene";

// The saved plan, as the chip in the toolbar describes it. Before: tokens and pool only, the payment
// is "planned at arrival". After: the chosen planets are in the plan.
test("leadership prep: the saved plan names the payment", async ({ page }, testInfo) => {
  await openScene(page, acceptingScript);
  await prepareAndSave(page);
  await page.waitForTimeout(200);
  await shot(page, testInfo, `${prefix}-2-saved-plan`, { of: page.getByTestId("secondary-prep-chips"), pad: 16 });
});
