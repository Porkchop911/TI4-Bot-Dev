import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { loneStrategic, openLoneGame } from "./lone";

// Switched off: the same lone decision stays on the turn bar and is not sent.
test("setting: auto off asks even for a lone move", async ({ page }, testInfo) => {
  const { advance, submissions } = await openLoneGame(page);
  await page.getByTestId("auto-submit-btn").click();
  await expect(page.getByTestId("auto-submit-btn")).toContainText("Auto off");
  advance(1, null);
  advance(2, loneStrategic("n-off"));
  await page.getByTestId("turn-action-bar").waitFor();
  await page.waitForTimeout(1200);
  expect(submissions).toHaveLength(0);
  await expect(page.getByTestId("corner-toast")).toHaveCount(0);
  await shot(page, testInfo, "4-setting-off");
});
