import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReady, prefix } from "./scene";

// Picking in the list: one planet is not enough (Confirm stays off), the second enables it.
test("diplomacy ready: one picked, then both", async ({ page }, testInfo) => {
  await openReady(page);
  if (prefix === "before") {
    await page.getByTestId("planet-chip-jord").click();
    await shot(page, testInfo, `${prefix}-4a-one-picked`);
    return;
  }
  await page.getByTestId("ready-planet-chip-jord").click();
  await expect(page.getByTestId("ready-bar-confirm")).toBeDisabled();
  await shot(page, testInfo, `${prefix}-4a-one-picked`);
  await page.getByTestId("ready-planet-chip-tarmann").click();
  await expect(page.getByTestId("ready-bar-confirm")).toBeEnabled();
  await shot(page, testInfo, `${prefix}-4b-both-picked`);
});
