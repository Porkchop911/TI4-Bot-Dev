import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openLoneGame, PASS, STRATEGIC, TACTICAL, turnMenu } from "./lone";

// A normal menu (strategic plus tactical and pass) is a real choice: nothing is sent, no toast.
test("a normal turn menu is still asked", async ({ page }, testInfo) => {
  const { advance, submissions } = await openLoneGame(page);
  advance(1, null);
  advance(2, turnMenu([STRATEGIC, TACTICAL, PASS], "n-normal"));
  await page.getByTestId("turn-action-bar").waitFor();
  await page.waitForTimeout(1200);
  expect(submissions).toHaveLength(0);
  await expect(page.getByTestId("corner-toast")).toHaveCount(0);
  await shot(page, testInfo, "5-normal-menu");
});
