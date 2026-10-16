import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { noHorizontalPageScroll, openObjectives, openScene, SIZES, viewer, type SizeName } from "./scene";

// The Objectives dialog while a home planet is lost: a banner says why nothing public can be
// scored, and met objectives read "Met · home system lost" instead of a pulsing "Ready".
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`objectives home lost ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, "home-lost");
    await openObjectives(page, size);
    const modal = page.getByTestId("objectives-modal");
    const banner = modal.getByTestId("home-system-lost");
    await expect(banner).toContainText("You cannot score public objectives");
    await expect(banner).toContainText("Archon Ren (held by");
    await expect(banner).toContainText("rule 61.16");
    for (const id of ["push_boundaries", "unify_colonies"]) {
      await expect(modal.getByTestId(`status-blocked-${id}-${viewer}`)).toContainText("home system lost");
      await expect(modal.getByTestId(`status-ready-${id}-${viewer}`)).toHaveCount(0);
    }
    if (size === "390x844") expect(await noHorizontalPageScroll(page)).toBeLessThanOrEqual(0);
    await shot(page, testInfo, `1-home-lost-${size}`);
    await modal.getByTestId("objective-row-unify_colonies").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `1-home-lost-rows-${size}`);
  });
}

test("objectives home held desktop", async ({ page }, testInfo) => {
  await openScene(page, "desktop", "home-held");
  await openObjectives(page, "desktop");
  const modal = page.getByTestId("objectives-modal");
  await expect(modal.getByTestId("home-system-lost")).toHaveCount(0);
  await expect(modal.getByTestId(`status-ready-unify_colonies-${viewer}`)).toContainText("Ready (6/6)");
  await modal.getByTestId("objective-row-unify_colonies").scrollIntoViewIfNeeded();
  await shot(page, testInfo, "1-home-held-desktop");
});
