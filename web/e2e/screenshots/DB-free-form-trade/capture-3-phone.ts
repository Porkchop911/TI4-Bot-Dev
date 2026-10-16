import { test } from "@playwright/test";
import { click, openFreeDesk, shotDesk } from "./_free";

test("free-form trade: phone", async ({ page }, testInfo) => {
  await openFreeDesk(page, 390, 844);
  for (let i = 0; i < 3; i++) await click(page, "stage-give-tg-inc");
  for (let i = 0; i < 5; i++) await click(page, "stage-receive-tg-inc");
  await page.getByTestId("stage-status-custom").waitFor();
  await shotDesk(page, testInfo, "3-phone");
});
