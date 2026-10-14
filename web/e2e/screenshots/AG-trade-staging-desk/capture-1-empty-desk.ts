import { test } from "@playwright/test";
import { openDesk, shotDesk } from "./_trade";

// The propose desk before anything is staged: two columns, both purses, Propose disabled.
test("trade staging desk: empty", async ({ page }, testInfo) => {
  await openDesk(page);
  await page.getByTestId("stage-status-empty").waitFor();
  await shotDesk(page, testInfo, "1-empty-desk");
});
