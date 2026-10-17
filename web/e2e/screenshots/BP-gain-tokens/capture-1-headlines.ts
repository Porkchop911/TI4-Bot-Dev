import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { leadershipChoice, openChoice, prefix, singleChoice } from "./scene";

// The headline of a command-token gain names the real count. After the change the title reads
// "Gain N command tokens"; before it read the engine's one-token prompt.
const headline = (page: import("@playwright/test").Page) => page.getByTestId("choice-prompt");
const check = async (page: import("@playwright/test").Page, text: string) => {
  if (prefix === "after") await expect(headline(page)).toHaveText(text);
};

test("leadership primary: gain 3, desktop", async ({ page }, testInfo) => {
  await openChoice(page, leadershipChoice());
  await check(page, "Gain 3 command tokens — assign them to pools");
  await shot(page, testInfo, `${prefix}-3-leadership-desktop`);
});

test("leadership primary: gain 3, phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openChoice(page, leadershipChoice());
  await check(page, "Gain 3 command tokens — assign them to pools");
  await shot(page, testInfo, `${prefix}-4-leadership-phone`);
});

test("single token, desktop", async ({ page }, testInfo) => {
  await openChoice(page, singleChoice());
  await check(page, "Gain 1 command token — choose its pool");
  await shot(page, testInfo, `${prefix}-5-single-desktop`);
});
