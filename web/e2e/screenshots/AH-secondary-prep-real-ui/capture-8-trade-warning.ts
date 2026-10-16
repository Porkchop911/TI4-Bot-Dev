import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Trade: follow or skip only, with the replenish warning.
test("secondary prep: Trade warning", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Trade", card: "pok5trade" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("prepare-approximate").locator("summary").click();
  await shot(page, testInfo, "8-trade-warning");
});
