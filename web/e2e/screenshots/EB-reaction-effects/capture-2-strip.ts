import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { SIZES, prefix } from "./scene";
import { STRIP_SCENES } from "./strip-scenes";

// The active-effects strip in several situations, desktop and a phone. The same capture runs on the
// base commit with TI4_SHOT_PREFIX=before: the mocked view carries `active_effects` there too, and
// the base page ignores it.
for (const scene of STRIP_SCENES) {
  for (const size of SIZES) {
    test(`${prefix}-strip-${scene.key}-${size.label}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: size.width, height: size.height });
      await openMockedGame(page, {
        phase: scene.phase,
        players: [playerWithHand(), opponent],
        view: { active_effects: scene.effects },
      });
      await page.waitForTimeout(600);
      if (prefix === "after") {
        const strip = page.getByTestId("active-effects-strip");
        await expect(strip).toBeVisible();
        await expect(page.getByTestId("active-effect-chip")).toHaveCount(scene.effects.length);
        if (scene.expand) {
          await page.getByTestId("active-effects-toggle").click();
          await expect(page.getByTestId("active-effect-row")).toHaveCount(scene.effects.length);
        }
        const box = (await strip.boundingBox())!;
        expect(box.x).toBeGreaterThanOrEqual(0);
        expect(box.x + box.width).toBeLessThanOrEqual(size.width + 0.5);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
        // One line collapsed: a phone does not lose a third of its screen to it.
        if (!scene.expand) expect(box.height).toBeLessThanOrEqual(size.width < 500 ? 90 : 60);
      }
      await shot(page, testInfo, `${prefix}-strip-${scene.key}-${size.label}`, { of: page.locator(".app-shell__header"), pad: 0 });
    });
  }
}
