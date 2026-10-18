import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, actor, opponent, gainRelics } from "./relic";

// Dismissed relic toasts stay in the Recent notifications list.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`relic gains in the toast history ${size}`, async ({ page }, testInfo) => {
    await gainRelics(page, viewport, [
      { round: 3, player: opponent.id, relic: "shard", victory_points: 1 },
      { round: 3, player: actor, relic: "nanoforge" },
    ]);
    while ((await page.getByTestId("corner-toast").count()) > 0) {
      await page.getByTestId("corner-toast").last().click();
      await page.waitForTimeout(450);
    }
    await page.getByTestId(size === "desktop" ? "toast-history-toggle" : "toast-history-mobile-toggle").click();
    await expect(page.getByTestId("toast-history-item")).toHaveCount(2);
    await expect(page.getByTestId("toast-history-item").first()).toContainText("gained Nano-Forge");
    await shot(page, testInfo, `history-${size}`);
  });
}
