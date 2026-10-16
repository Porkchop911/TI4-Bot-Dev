import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAME_ID } from "../_shared/mockGame";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { openScene, playedImperial, SIZES, type SizeName, type Variant } from "./scene";

// The viewer plays Imperial and the engine asks nothing (nothing is scoreable): a corner notice
// says why, instead of the play looking swallowed.
const EXPECT: Partial<Record<Variant, string[]>> = {
  "home-lost": [
    "No public objective could be scored",
    "you do not control Archon Ren (held by",
    "in your home system (rule 61.16)",
  ],
  "none-met": ["No public objective could be scored: none is met", "Push Boundaries 1/2", "Unify the Colonies 5/6"],
};

for (const size of Object.keys(SIZES) as SizeName[]) {
  for (const [variant, parts] of Object.entries(EXPECT) as [Variant, string[]][]) {
    if (size !== "desktop" && variant !== "home-lost") continue;
    test(`imperial notice ${variant} ${size}`, async ({ page }, testInfo) => {
      const game = await openScene(page, size, variant);
      await page.waitForTimeout(500);
      game.send({ type: "event", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, entry: playedImperial("imperial-1") });
      const toast = page.locator('[data-testid="corner-toast"][data-toast-kind="notice"]');
      await toast.waitFor();
      await expect(toast.getByTestId("toast-notice-tag")).toHaveText("Imperial");
      for (const part of parts) await expect(toast).toContainText(part);
      await page.waitForTimeout(700); // the toast slides in
      await expect(toast).toHaveCSS("border-left-color", "rgb(245, 158, 11)");
      const viewport = SIZES[size];
      const box = (await toast.boundingBox())!;
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
      expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
      await shot(page, testInfo, `2-notice-${variant}-${size}`);
    });
  }
}

test("imperial notice silent when an objective is met", async ({ page }) => {
  const game = await openScene(page, "desktop", "home-held");
  await page.waitForTimeout(500);
  game.send({ type: "event", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, entry: playedImperial("imperial-2") });
  await page.waitForTimeout(1200);
  await expect(page.locator('[data-testid="corner-toast"][data-toast-kind="notice"]')).toHaveCount(0);
});
