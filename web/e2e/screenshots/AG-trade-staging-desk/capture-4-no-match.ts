import { test } from "@playwright/test";
import { openDesk, click, shotDesk } from "./_trade";

// Three trade goods for three is not on the server's list: the desk says so, says why, and offers
// the nearest listed deals as one-click suggestions. Propose stays disabled.
test("trade staging desk: no matching deal", async ({ page }, testInfo) => {
  await openDesk(page);
  for (let i = 0; i < 3; i++) await click(page, "stage-give-tg-inc");
  for (let i = 0; i < 3; i++) await click(page, "stage-receive-tg-inc");
  await page.getByTestId("stage-status-invalid").waitFor();
  await shotDesk(page, testInfo, "4-no-match-suggestions");
});
