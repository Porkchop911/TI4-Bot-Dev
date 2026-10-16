import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { loneStrategic, openLoneGame } from "./lone";

// Undo lands on the lone decision (something to redo): it is asked, so undo stays possible.
test("a lone decision reached by undo is asked", async ({ page }, testInfo) => {
  const { advance, submissions } = await openLoneGame(page);
  advance(2, null);
  advance(1, loneStrategic("n-after-undo"), 1);
  await page.getByTestId("turn-action-bar").waitFor();
  await page.waitForTimeout(1200);
  expect(submissions).toHaveLength(0);
  await expect(page.getByTestId("corner-toast")).toHaveCount(0);
  await shot(page, testInfo, "6-after-undo");
});
