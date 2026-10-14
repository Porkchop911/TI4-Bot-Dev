import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, SIZES, type SizeName } from "./scene";

// The player sheet's private hand: each countable secret shows have / threshold, what counts, and a
// bar; a met one turns green; the event secret gets a one-line note instead of a bar.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`player sheet secrets ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size);
    if (size !== "desktop") {
      await page.getByTestId("player-sheet-toggle").click();
      await page.waitForTimeout(400);
    }
    const ctr = page.getByTestId("secret-objective-item-ctr");
    await expect(ctr).toContainText("4 / 6 systems with at least one of your ships");
    await expect(page.getByTestId("secret-progress-eap")).toHaveAttribute("data-satisfied", "true");
    await expect(page.getByTestId("secret-objective-item-hrm")).toContainText(
      "9 / 12 resources on planets you control",
    );
    await expect(page.getByTestId("secret-progress-note-pe")).toBeVisible();
    // Only the viewer's own card has a private hand.
    await expect(page.getByTestId("private-hand-section")).toHaveCount(1);
    const list = ctr.locator("xpath=../..");
    await page.getByTestId("secret-objective-item-pe").scrollIntoViewIfNeeded();
    if (size === "390x844") {
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
    }
    // Desktop: the whole sidebar, scrolled to the hand; phone: the secret list in the drawer.
    const target = size === "desktop" ? page.getByTestId("player-sheet-drawer") : list;
    await shot(page, testInfo, `1-player-sheet-${size}`, { of: target, pad: size === "desktop" ? 0 : 12 });
  });
}
