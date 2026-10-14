import { test } from "@playwright/test";
import { openDesk, click, shotDesk } from "./_trade";

// A commodity swap picked from "Quick deals": the desk stages both sides, shows the net value and
// the action bar (Offer nothing / Clear desk / Propose Deal) is ready.
test("trade staging desk: propose ready, quick deals open", async ({ page }, testInfo) => {
  await openDesk(page);
  await page.getByTestId("quick-deals").locator("summary").click();
  await click(page, "trade-opt-cc2");
  await page.getByTestId("stage-status-valid").waitFor();
  await shotDesk(page, testInfo, "5-propose-ready-quick-deals");
});
