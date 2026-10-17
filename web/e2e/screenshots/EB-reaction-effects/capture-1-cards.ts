import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SCENES } from "./cards";
import { SIZES, openScene, prefix } from "./scene";

// One reaction window per card with an effect preview, desktop and a phone. The same capture runs
// on the base commit with TI4_SHOT_PREFIX=before: there the dialog ignores the new `previews`.
for (const scene of SCENES) {
  for (const size of SIZES) {
    test(`${prefix}-${scene.key}-${size.label}`, async ({ page }, testInfo) => {
      await openScene(page, scene, size);
      const bar = page.getByTestId("reaction-status-bar");
      if (prefix === "after") {
        // The preview is a plain sentence under the card text, not raw ids.
        const effect = page.getByTestId("reaction-row-effect");
        await expect(effect).toBeVisible();
        const text = (await effect.textContent()) ?? "";
        expect(text.length).toBeGreaterThan(20);
        expect(text).not.toMatch(/_|undefined|null/);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
        // The Play and Pass buttons stay reachable inside the viewport.
        const play = await page.getByTestId(`play-reaction-btn-reaction:hacan:${scene.event}:${scene.relation}`).boundingBox();
        expect(play).not.toBeNull();
      }
      await shot(page, testInfo, `${prefix}-${scene.key}-${size.label}`, { of: bar, pad: 8 });
    });
  }
}
