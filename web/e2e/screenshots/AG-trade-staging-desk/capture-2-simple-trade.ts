import { test } from "@playwright/test";
import { openDesk, click, shotDesk } from "./_trade";

// Two trade goods for three, staged with the steppers: a listed deal, so Propose is enabled.
test("trade staging desk: a simple trade", async ({ page }, testInfo) => {
  await openDesk(page);
  for (let i = 0; i < 2; i++) await click(page, "stage-give-tg-inc");
  for (let i = 0; i < 3; i++) await click(page, "stage-receive-tg-inc");
  await page.getByTestId("stage-status-valid").waitFor();
  await shotDesk(page, testInfo, "2-simple-trade");
});
