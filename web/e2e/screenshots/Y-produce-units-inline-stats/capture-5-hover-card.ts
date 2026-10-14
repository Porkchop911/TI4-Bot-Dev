import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { shot } from "../_shared/shot";
import { productionGame } from "./fixtures";

// The info card keeps the full ability text; hover or focus opens it, a tap on a phone.
test("produce units: hover card, desktop", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame("sol", ["dn2", "ac2"]));
  await page.getByTestId("production-builder-drawer").waitFor();
  await hoverShot(page, testInfo, "5-hover-flagship", page.getByTestId("unit-info-flagship"), {
    tooltip: page.getByTestId("unit-info-flagship-card"),
  });
});

test.describe("phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  test("produce units: info card, phone", async ({ page }, testInfo) => {
    await openMockedGame(page, productionGame("sol", ["dn2", "ac2"]));
    await page.getByTestId("production-builder-drawer").waitFor();
    await page.getByTestId("unit-info-carrier").tap();
    await page.getByTestId("unit-info-carrier-card").waitFor();
    await shot(page, testInfo, "5b-card-phone");
  });
});
