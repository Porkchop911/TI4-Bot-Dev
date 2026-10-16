import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "../AH-secondary-prep-real-ui/prep";
import { board, prefix } from "./scene";

// Preparing the Diplomacy secondary while another seat resolves the card. Before: the first planet,
// then a second question for the second. After: both planets in one go on the same ready bar;
// confirming records the pair in the plan and sends nothing.
test("diplomacy ready: preparing the secondary", async ({ page }, testInfo) => {
  await openWaiting(page, { name: "Diplomacy", card: "pok2diplomacy", board });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  if (prefix === "before") {
    await page.getByTestId("planet-selection-bar").waitFor();
    await page.getByTestId("planet-jord").click();
    await shot(page, testInfo, `${prefix}-7-prepare`);
    return;
  }
  await page.getByTestId("ready-planets-bar").waitFor();
  await page.getByTestId("planet-jord").click();
  await page.getByTestId("ready-planet-chip-tarmann").click();
  await shot(page, testInfo, `${prefix}-7-prepare`);
});
