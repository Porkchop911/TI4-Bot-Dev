import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openFactionLobby } from "./scene";

test.use({ viewport: { width: 390, height: 844 } });

const noSideScroll = async (page: import("@playwright/test").Page) =>
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);

test("phone: picker closed", async ({ page }, testInfo) => {
  await openFactionLobby(page);
  const box = await page.getByTestId("new-bot-faction-button").boundingBox();
  expect(box?.height ?? 0).toBeGreaterThanOrEqual(40);
  await noSideScroll(page);
  await shot(page, testInfo, "8-phone-picker-closed");
});

test("phone: picker open", async ({ page }, testInfo) => {
  await openFactionLobby(page, { bots: ["hacan"] });
  await page.getByTestId("new-bot-faction-button").click();
  const list = await page.getByRole("listbox").boundingBox();
  expect((list?.x ?? 0) + (list?.width ?? 0)).toBeLessThanOrEqual(390);
  for (const option of await page.getByRole("option").all())
    expect((await option.boundingBox())?.height ?? 0).toBeGreaterThanOrEqual(40);
  await noSideScroll(page);
  await shot(page, testInfo, "9-phone-picker-open");
});

test("phone: seats with chosen factions", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 1300 });
  await openFactionLobby(page, { bots: ["xxcha", null, "l1z1x"] });
  await expect(page.getByTestId("seat-faction-picker-5-button")).toContainText("L1Z1X");
  await noSideScroll(page);
  await shot(page, testInfo, "10-phone-seats");
});

test("phone: duplicate refused", async ({ page }, testInfo) => {
  await openFactionLobby(page, { hiddenHolder: { faction: "sol", name: "Rule bot 9" } });
  await page.getByTestId("new-bot-faction-button").click();
  await page.getByTestId("new-bot-faction-option-sol").click();
  await page.getByTestId("add-smart-bot-button").click();
  await expect(page.getByTestId("lobby-error")).toContainText("already taken by Rule bot 9");
  await shot(page, testInfo, "11-phone-duplicate-refused");
});
