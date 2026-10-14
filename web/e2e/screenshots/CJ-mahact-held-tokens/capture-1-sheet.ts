import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { open } from "./scene";

test("desktop: the Mahact sheet lists the tokens it holds", async ({ page }, testInfo) => {
  await open(page, { width: 1440, height: 900 });
  const cards = page.getByTestId("player-card");
  const mahact = cards.nth(0);
  const held = mahact.getByTestId("player-held-tokens");
  await expect(held).toBeVisible();
  await expect(held.getByTestId("player-held-token")).toHaveText([/Alex/, /Casey/]);
  await expect(page.getByTestId("player-held-tokens")).toHaveCount(1);
  await shot(page, testInfo, "desktop-sheet");
  await shot(page, testInfo, "desktop-sheet-crop", { of: mahact, pad: 8 });
});

test("desktop: no chips while the pool is empty", async ({ page }, testInfo) => {
  await open(page, { width: 1440, height: 900 }, undefined, []);
  await expect(page.getByTestId("player-held-tokens")).toHaveCount(0);
  await shot(page, testInfo, "desktop-empty-pool");
});

test("phone: the drawer shows the same chips", async ({ page }, testInfo) => {
  await open(page, { width: 390, height: 844 });
  await page.getByTestId("player-sheet-toggle").click();
  const held = page.getByTestId("player-held-tokens");
  await expect(held).toBeVisible();
  await shot(page, testInfo, "phone-drawer");
});
