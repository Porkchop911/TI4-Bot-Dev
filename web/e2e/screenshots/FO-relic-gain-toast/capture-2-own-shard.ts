import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, actor, gainRelics, toasts } from "./relic";

// The Shard of the Throne is worth a point the moment it arrives: the toast says so.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`own Shard of the Throne ${size}`, async ({ page }, testInfo) => {
    await gainRelics(page, viewport, [{ round: 3, player: actor, relic: "shard", victory_points: 1 }]);
    const toast = page.locator('[data-toast-kind="relic"]');
    await expect(toast).toContainText("gained Shard of the Throne");
    await expect(toast).toContainText("+1 victory point on gain");
    await expect(toast).not.toContainText("no victory point");
    await shot(page, testInfo, `own-shard-${size}`, { of: toasts(page), pad: 24 });
  });
}
