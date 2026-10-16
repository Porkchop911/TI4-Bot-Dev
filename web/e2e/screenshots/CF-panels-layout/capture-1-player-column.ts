import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openTurn, prefix } from "./scene";

test("desktop: the last player sheet scrolls clear of the action bar", async ({ page }, testInfo) => {
  await openTurn(page, { width: 1280, height: 700 });
  const panel = page.getByTestId("player-sheet-panel");
  await panel.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  await page.waitForTimeout(200);
  if (prefix === "after") {
    const last = (await page.getByTestId("player-card").last().boundingBox())!;
    const bar = (await page.getByTestId("turn-action-bar").boundingBox())!;
    expect(last.y + last.height).toBeLessThanOrEqual(bar.y + 1);
  }
  await shot(page, testInfo, `${prefix}-desktop-column-scrolled`);
});

test("phone: the drawer scrolls to the last sheet above the buttons, and closes", async ({ page }, testInfo) => {
  await openTurn(page, { width: 390, height: 844 });
  await page.getByTestId("player-sheet-toggle").click();
  const drawer = page.getByTestId("player-sheet-drawer");
  await drawer.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  await page.waitForTimeout(300);
  if (prefix === "after") {
    const last = (await page.getByTestId("player-card").last().boundingBox())!;
    const buttons = (await page.locator(".app-shell__mobile-actions").boundingBox())!;
    expect(last.y + last.height).toBeLessThanOrEqual(buttons.y + 1);
    await expect(page.getByTestId("player-sheet-close")).toBeVisible();
  }
  await shot(page, testInfo, `${prefix}-phone-drawer-scrolled`);
  if (prefix === "before") return;
  await page.getByTestId("player-sheet-close").click();
  await expect(drawer).not.toHaveClass(/app-shell__drawer--open/);
  // Tapping the map outside an open drawer closes it as well.
  await page.getByTestId("player-sheet-toggle").click();
  await expect(drawer).toHaveClass(/app-shell__drawer--open/);
  await page.mouse.click(195, 60);
  await expect(drawer).not.toHaveClass(/app-shell__drawer--open/);
});
