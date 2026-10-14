import { test } from "@playwright/test";
import { openDesk, click, shotDesk } from "./_trade";

// An action card (listed only because the engine allows it) for the partner's promissory note,
// each item with its card text.
test("trade staging desk: promissory note and action card", async ({ page }, testInfo) => {
  await openDesk(page);
  await click(page, "stage-give-ac-bribery");
  await click(page, "stage-receive-note-political_secret:jolnar");
  await page.getByTestId("stage-status-valid").waitFor();
  await shotDesk(page, testInfo, "3-note-and-card");
});
