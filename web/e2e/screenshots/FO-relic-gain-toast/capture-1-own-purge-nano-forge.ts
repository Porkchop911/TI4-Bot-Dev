import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, actor, gainRelics, toasts } from "./relic";

// The alpha report: three hazardous fragments purged, the relic Nano-Forge, and no victory point.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`own fragment purge gains Nano-Forge ${size}`, async ({ page }, testInfo) => {
    await gainRelics(page, viewport, [{ round: 3, player: actor, relic: "nanoforge" }]);
    const toast = page.locator('[data-toast-kind="relic"]');
    await expect(toast).toContainText("gained Nano-Forge");
    await expect(toast).toContainText("Attach this card to a non-legendary, non-home planet you control");
    await expect(toast).toContainText("no victory point on gain");
    await expect(toast).toHaveClass(/auto-resolve-toast--own/);
    const box = (await toast.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
    expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
    await shot(page, testInfo, `own-nano-forge-${size}`, { of: toasts(page), pad: 24 });
  });
}
