import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "./prep";

// Construction: the real structure placement bar, approximate until the engine offers its sites.
test("secondary prep: Construction site", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Construction", card: "pok4construction" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("planet-wellon").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await page.getByTestId("prepare-approximate").locator("summary").click();
  await shot(page, testInfo, "6-construction");
});
