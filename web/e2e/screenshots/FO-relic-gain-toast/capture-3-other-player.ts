import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, opponent, gainRelics, toasts } from "./relic";

// Another player's gain: their name and seat colour, no highlight, no "no victory point" note.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`another player's relic gain ${size}`, async ({ page }, testInfo) => {
    await gainRelics(page, viewport, [{ round: 3, player: opponent.id, relic: "codex" }]);
    const toast = page.locator('[data-toast-kind="relic"]');
    await expect(toast).toContainText("gained The Codex");
    await expect(toast).not.toHaveClass(/auto-resolve-toast--own/);
    await expect(toast).not.toContainText("victory point");
    await shot(page, testInfo, `other-player-${size}`, { of: toasts(page), pad: 24 });
  });
}
