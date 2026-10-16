import { test } from "@playwright/test";
import { openDesk, click, shotDesk } from "./_trade";

// Hacan-style note asks: buy the partner's note for commodities (cp), staged on the desk, and the
// promissory tab of Quick deals listing every ask shape (np, cp, pc, nn, cn) with plain wording.
test("trade staging desk: Hacan note ask shapes", async ({ page }, testInfo) => {
  await openDesk(page, undefined, 1500);
  for (let i = 0; i < 3; i++) await click(page, "stage-give-cm-inc");
  await click(page, "stage-receive-note-political_secret:jolnar");
  await page.getByTestId("stage-status-valid").waitFor();
  await page.getByTestId("quick-deals").locator("summary").click();
  await click(page, "trade-tab-promissory");
  await shotDesk(page, testInfo, "8-hacan-note-asks");
});
