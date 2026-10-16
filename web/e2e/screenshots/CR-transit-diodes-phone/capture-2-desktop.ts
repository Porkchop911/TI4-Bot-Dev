import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openTransit, prefix } from "./scene";

// The same panel on a desktop: centred above the map, same steps.
test("after-desktop", async ({ browser, baseURL }, testInfo) => {
  test.skip(prefix !== "after", "after shots need the stepwise panel");
  const { context, page } = await openTransit(browser, { label: "desktop", width: 1440, height: 900 }, baseURL);
  const panel = page.getByTestId("transit-panel");
  await panel.waitFor();
  await page.getByTestId("transit-force-18|jord|infantry").click();
  await page.getByTestId("transit-destination-rarron").scrollIntoViewIfNeeded();
  await page.getByTestId("transit-destination-rarron").click();
  await expect(page.getByTestId("transit-summary")).toHaveText(
    "Move 1 Infantry: Jord (#18 [Mecatol Rex]) → Rarron (#29 [Qucen'n / Rarron])",
  );
  await shot(page, testInfo, "after-6-desktop");
  await context.close();
});
