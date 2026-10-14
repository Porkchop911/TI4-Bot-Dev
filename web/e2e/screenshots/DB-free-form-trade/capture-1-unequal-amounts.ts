import { test } from "@playwright/test";
import { click, openFreeDesk, shotDesk } from "./_free";

// Give 3 trade goods, ask for 5 trade goods and 1 commodity: no listed deal, still proposable.
test("free-form trade: unequal amounts", async ({ page }, testInfo) => {
  await openFreeDesk(page);
  for (let i = 0; i < 3; i++) await click(page, "stage-give-tg-inc");
  for (let i = 0; i < 5; i++) await click(page, "stage-receive-tg-inc");
  await click(page, "stage-receive-cm-inc");
  await page.getByTestId("stage-status-custom").waitFor();
  await shotDesk(page, testInfo, "1-unequal-amounts");
});
