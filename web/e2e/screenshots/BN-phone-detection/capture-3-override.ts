import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { newDevicePage, openScene, prefix } from "./scene";

// The manual Layout setting: in the menu sheet (Auto / Phone / Desktop) and as ?layout=...
test.skip(prefix === "before", "the before side has no layout setting");

test("menu layout row and override in desktop-site mode", async ({ browser, baseURL }, testInfo) => {
  const { context, page } = await newDevicePage(browser, "desktop-site", baseURL);
  await openScene(page, "waiting");
  await page.getByTestId("top-menu-button").click();
  await page.getByTestId("top-menu-layout").waitFor();
  await shot(page, testInfo, "after-menu-layout-desktop-site");
  await page.getByTestId("top-menu-layout-desktop").click();
  await expect(page.locator("html")).toHaveAttribute("data-layout", "desktop");
  expect(await page.evaluate(() => localStorage.getItem("ti4.layout.v1"))).toBe("desktop");
  await context.close();
});

test("?layout=compact on a desktop and ?layout=auto clears it", async ({ browser, baseURL }, testInfo) => {
  const { context, page } = await newDevicePage(browser, "desktop", baseURL);
  await openScene(page, "waiting");
  await expect(page.locator("html")).toHaveAttribute("data-layout", "desktop");
  await page.goto("/games/screenshot-game?layout=compact");
  await expect(page.locator("html")).toHaveAttribute("data-layout", "compact");
  expect(page.url()).not.toContain("layout=");
  await page.getByTestId("ti4-board-svg").waitFor();
  await shot(page, testInfo, "after-override-compact-desktop");
  await page.goto("/games/screenshot-game?layout=auto");
  await expect(page.locator("html")).toHaveAttribute("data-layout", "desktop");
  await context.close();
});
