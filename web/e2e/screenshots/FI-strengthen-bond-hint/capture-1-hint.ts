import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, SIZES, type SizeName } from "./scene";

// Strengthen Bonds at 0 / 1 now says that only face-up play-area notes count, not the hand.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`player sheet strengthen bonds ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size);
    if (size !== "desktop") {
      await page.getByTestId("player-sheet-toggle").click();
      await page.waitForTimeout(400);
    }
    const item = page.getByTestId("secret-objective-item-sb");
    await expect(item).toContainText("0 / 1 rivals' promissory notes in your play area");
    await expect(page.getByTestId("secret-progress-hint-sb")).toContainText("Notes in your hand do not");
    if (size === "390x844") {
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
    }
    await item.scrollIntoViewIfNeeded();
    const target = size === "desktop" ? page.getByTestId("player-sheet-drawer") : item;
    await shot(page, testInfo, `1-player-sheet-${size}`, { of: target, pad: size === "desktop" ? 0 : 16 });
  });
}

for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`objectives dialog strengthen bonds ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size);
    await page
      .getByTestId(size === "desktop" ? "objectives-modal-button" : "objectives-mobile-button")
      .click();
    await page.getByTestId("objectives-modal").waitFor();
    await expect(page.getByTestId("objectives-modal").getByTestId("secret-progress-hint-sb")).toBeVisible();
    await page.getByTestId("secret-objectives-header").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `2-objectives-${size}`);
  });
}
