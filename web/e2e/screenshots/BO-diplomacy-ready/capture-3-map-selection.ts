import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReady, prefix } from "./scene";

// Picking on the map. Before: a tap on Wellon selects it and the bar asks Confirm for that one planet.
// After: taps on Wellon and Jord on the map mark both with a green ring and a check, on the map and
// in the list; one Confirm readies both.
test("diplomacy ready: planets picked on the map", async ({ page }, testInfo) => {
  await openReady(page);
  if (prefix === "before") {
    await page.getByTestId("planet-wellon").click();
    await page.getByTestId("confirm-planet-btn").waitFor();
  } else {
    await page.getByTestId("planet-wellon").click();
    await page.getByTestId("planet-jord").click();
    await page.getByTestId("ready-bar-confirm").waitFor();
  }
  await shot(page, testInfo, `${prefix}-3-map-selection`);
});
