import { test } from "@playwright/test";
import { click, openFreeDesk, shotDesk } from "./_free";

// The steppers stop at the purse (3 trade goods, 4 commodities held), so a deal one side cannot pay
// is never sent; here the partner's 2 commodities are staged along with a 4-commodity gift.
test("free-form trade: steppers stop at the purse", async ({ page }, testInfo) => {
  await openFreeDesk(page);
  for (let i = 0; i < 4; i++) await click(page, "stage-give-cm-inc");
  for (let i = 0; i < 2; i++) await click(page, "stage-receive-cm-inc");
  await page.getByTestId("stage-status-custom").waitFor();
  await shotDesk(page, testInfo, "2-commodities-4-for-2");
});
