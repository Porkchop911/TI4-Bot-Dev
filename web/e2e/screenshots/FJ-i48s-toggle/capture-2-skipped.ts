import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { i48sOffer, openScene } from "./scene";

// Switch it off in the player sheet; the next activation's offer is declined by the client.
test("I48S skipped after switching off", async ({ page }, testInfo) => {
  const { advance, submissions } = await openScene(page);
  const button = page.getByTestId("i48s-ask-btn");
  await expect(button).toContainText("I48S ask");
  await shot(page, testInfo, "2-sheet-ask", { of: page.getByTestId("player-sheet-panel").locator("h2").locator(".."), pad: 40 });
  await button.click();
  await expect(button).toContainText("I48S skipped");
  await shot(page, testInfo, "3-sheet-skipped", { of: page.getByTestId("player-sheet-panel").locator("h2").locator(".."), pad: 40 });
  advance(1, null);
  await page.waitForTimeout(300);
  advance(2, i48sOffer("i48s-2"));
  await expect.poll(() => submissions.length).toBe(1);
  expect(JSON.stringify(submissions[0])).toContain("decline");
  await expect(page.getByTestId("corner-toast")).toContainText("I48S skipped");
  await shot(page, testInfo, "4-declined-toast");
});

// Ticking the switch inside the offer itself turns it off for next time.
test("I48S switch inside the offer", async ({ page }, testInfo) => {
  const { advance, submissions } = await openScene(page);
  advance(1, null);
  advance(2, i48sOffer("i48s-3"));
  await page.getByTestId("i48s-switch-input").check();
  await expect(page.getByTestId("i48s-switch-input")).toBeChecked();
  await shot(page, testInfo, "5-switch-ticked");
  expect(submissions).toHaveLength(0);
});

// After an undo that lands on the offer, it is asked again even with the switch off.
test("I48S asked again after an undo", async ({ page }, testInfo) => {
  const { advance, submissions } = await openScene(page);
  await page.getByTestId("i48s-ask-btn").click();
  advance(3, null);
  advance(2, i48sOffer("i48s-4"), 1);
  await page.waitForTimeout(1200);
  expect(submissions).toHaveLength(0);
  await expect(page.getByTestId("i48s-switch")).toBeVisible();
  await shot(page, testInfo, "6-after-undo");
});
