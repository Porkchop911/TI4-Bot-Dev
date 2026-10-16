import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, prefix, warfareScript } from "./scene";

// Warfare, prepare mode. After: following opens the real production builder on the engine's build
// list for the home system (units, costs, capacity, fleet supply), with two builds staged. Before:
// the build list is only known to the engine, so Warfare could only be followed or skipped.
test("secondary prep: Warfare production", async ({ page }, testInfo) => {
  await openScene(page, { name: "Warfare", card: "pok6warfare", script: prefix === "before" ? null : warfareScript });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  if (prefix === "before") {
    await page.getByTestId("secondary-prep-chip").waitFor();
    console.log("warfare before: no production step; the plan is just 'Follow Warfare'");
    await shot(page, testInfo, "before-2-warfare-prepare");
    return;
  }
  await page.getByTestId("production-builder-drawer").waitFor();
  await page.getByTestId("prepare-as-of-now").waitFor();
  await page.getByTestId("produce-unit-btn-build|infantry|2").click();
  await page.getByTestId("produce-unit-btn-build|carrier|1").click();
  // The build list is longer than the screen: bring the staged ground forces into view with the ships.
  await page.getByTestId("produce-option-build|infantry|2").scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
  await shot(page, testInfo, "after-2-warfare-prepare");
});
