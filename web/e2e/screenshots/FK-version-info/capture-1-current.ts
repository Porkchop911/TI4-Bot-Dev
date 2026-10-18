import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, openVersions, startGame } from "./versions";

// A new offline game plays the newest bot and rules: nothing is flagged.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`current version ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await startGame(page);
    await openVersions(page, size);
    const rules = page.getByTestId("version-rules");
    await expect(rules).toHaveAttribute("data-older", "false");
    await expect(rules).toContainText(/newest this build knows: \d+/);
    await expect(page.getByTestId("version-rules-note")).toHaveCount(0);
    await expect(page.getByTestId("version-bot")).toContainText(/version \d+/);
    await expect(page.getByTestId("version-bot-note")).toHaveCount(0);
    await expect(page.getByTestId("version-web-commit")).not.toBeEmpty();
    await expect(page.getByTestId("version-engine")).toBeVisible();
    await shot(page, testInfo, `1-current-${size}`);
  });
}
