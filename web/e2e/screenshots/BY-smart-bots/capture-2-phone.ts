import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBotLobby } from "./scene";

// The host on a phone (390 px wide).
const BOTH = ["random", "smart"];

test.use({ viewport: { width: 390, height: 1200 } });

test("phone: host with open seats", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: BOTH, players: 5, guest: true });
  for (const id of ["add-random-bot-button", "fill-random-bots-button", "add-smart-bot-button", "fill-smart-bots-button"]) {
    const box = await page.getByTestId(id).boundingBox();
    expect(box?.height ?? 0).toBeGreaterThanOrEqual(40);
    expect((box?.x ?? 0) + (box?.width ?? 0)).toBeLessThanOrEqual(390);
  }
  await shot(page, testInfo, "5-phone-open");
});

test("phone: filled with rule bots", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: BOTH, players: 5, guest: true });
  await page.getByTestId("fill-smart-bots-button").click();
  await expect(page.getByTestId("bot-badge-5")).toHaveText("Rule bot");
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await shot(page, testInfo, "6-phone-filled");
});

test("guest sees the rule bot badges but no host controls", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: BOTH, viewer: "guest_1", initialBots: 2, initialKind: "smart" });
  await expect(page.getByTestId("bot-badge-3")).toHaveText("Rule bot");
  await expect(page.getByTestId("add-smart-bot-button")).toHaveCount(0);
  await shot(page, testInfo, "7-guest");
});
