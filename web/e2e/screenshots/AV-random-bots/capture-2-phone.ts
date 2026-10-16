import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openBotLobby } from "./scene";

// The host on a phone (390 px wide), the way the user will try it. TI4_SHOT_PREFIX=before: the
// base commit's lobby on the same mocked server.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const before = prefix === "before";

test.use({ viewport: { width: 390, height: 1100 } });

test("phone: host with open seats", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], players: 5, guest: true });
  if (!before) await expect(page.getByTestId("fill-random-bots-button")).toBeVisible();
  await shot(page, testInfo, `${prefix}-5-phone-open`);
});

test("phone: host after filling", async ({ page }, testInfo) => {
  await openBotLobby(page, { botKinds: ["random"], players: 5, guest: true, initialBots: before ? 3 : 0 });
  if (!before) {
    await page.getByTestId("fill-random-bots-button").click();
    await expect(page.getByTestId("bot-badge-5")).toBeVisible();
    // No horizontal scrolling on a phone.
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(overflow).toBeLessThanOrEqual(0);
  }
  await shot(page, testInfo, `${prefix}-6-phone-filled`);
});

test("phone: buttons are touch sized", async ({ page }) => {
  test.skip(before, "the buttons do not exist on the base commit");
  await openBotLobby(page, { botKinds: ["random"], players: 5, guest: true });
  for (const id of ["add-random-bot-button", "fill-random-bots-button"]) {
    const box = await page.getByTestId(id).boundingBox();
    expect(box?.height ?? 0).toBeGreaterThanOrEqual(40);
    expect(box?.width ?? 0).toBeGreaterThanOrEqual(120);
    expect((box?.x ?? 0) + (box?.width ?? 0)).toBeLessThanOrEqual(390);
  }
});
