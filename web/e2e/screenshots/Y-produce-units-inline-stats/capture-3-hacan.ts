import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { productionGame } from "./fixtures";

// Hacan without upgrades: base cards and the faction's own flagship and mech.
test("produce units: Hacan, desktop", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame("hacan", []));
  await page.getByTestId("production-builder-drawer").waitFor();
  await shot(page, testInfo, "3-hacan-desktop");
});

test.describe("phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  test("produce units: Hacan, phone width", async ({ page }, testInfo) => {
    await openMockedGame(page, productionGame("hacan", []));
    await page.getByTestId("production-builder-drawer").waitFor();
    await shot(page, testInfo, "3b-hacan-phone");
  });
});
