import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// After the pick the plan is saved at once; the chip says Prepared and names it.
test("secondary prep: saved", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await page.locator('[data-testid^="tech-card-"][data-selectable="true"]').first().click();
  await page.getByTestId("confirm-research-btn").click();
  await page.getByTestId("secondary-prep-chip").filter({ hasText: "Prepared" }).waitFor();
  await shot(page, testInfo, "4-prepared");
});
