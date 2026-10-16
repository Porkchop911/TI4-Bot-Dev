import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openLimitDialog, rows } from "./scene";

export function flow(prefix: string, phone: boolean) {
  const noSideScroll = async (page: Page) =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);
  const insideViewport = async (page: Page) => {
    const box = await page.getByTestId("mygames-dialog").boundingBox();
    const vp = page.viewportSize()!;
    expect((box?.x ?? 0) + (box?.width ?? 0)).toBeLessThanOrEqual(vp.width);
    expect((box?.y ?? 0) + (box?.height ?? 0)).toBeLessThanOrEqual(vp.height);
  };
  test(`${prefix}: list with a not-yours row`, async ({ page }, testInfo) => {
    await openLimitDialog(page);
    await expect(page.getByRole("heading", { name: "You have too many unfinished games" })).toBeVisible();
    await expect(rows(page)).toHaveCount(3);
    await expect(rows(page).nth(2).getByTestId("mygames-not-yours")).toBeVisible();
    await expect(rows(page).nth(2).getByText("Terminate", { exact: true })).toHaveCount(0);
    if (phone) await insideViewport(page);
    await noSideScroll(page);
    await shot(page, testInfo, `${prefix === "desktop" ? "1" : "4"}-${prefix}-list`);
  });
  test(`${prefix}: confirm state`, async ({ page }, testInfo) => {
    await openLimitDialog(page);
    await rows(page).nth(0).getByText("Terminate", { exact: true }).click();
    await expect(rows(page).nth(0).getByText("Really end this game?")).toBeVisible();
    if (phone) {
      const yes = await rows(page).nth(0).getByRole("button", { name: "Yes" }).boundingBox();
      expect(yes?.height ?? 0).toBeGreaterThanOrEqual(40);
      await insideViewport(page);
    }
    await noSideScroll(page);
    await shot(page, testInfo, `${prefix === "desktop" ? "2" : "5"}-${prefix}-confirm`);
  });
  test(`${prefix}: after terminating`, async ({ page }, testInfo) => {
    await openLimitDialog(page);
    await expect(page.getByText("Create game", { exact: true })).toBeDisabled();
    await rows(page).nth(0).getByText("Terminate", { exact: true }).click();
    await rows(page).nth(0).getByRole("button", { name: "Yes" }).click();
    await expect(rows(page)).toHaveCount(2);
    await expect(page.getByText("Create game", { exact: true })).toBeEnabled();
    await page.getByRole("button", { name: "Terminate", exact: true }).first().click();
    await page.getByRole("button", { name: "Yes" }).click();
    await expect(rows(page)).toHaveCount(1);
    await expect(page.getByTestId("mygames-not-yours")).toBeVisible();
    await noSideScroll(page);
    await shot(page, testInfo, `${prefix === "desktop" ? "3" : "6"}-${prefix}-after`);
  });
}
