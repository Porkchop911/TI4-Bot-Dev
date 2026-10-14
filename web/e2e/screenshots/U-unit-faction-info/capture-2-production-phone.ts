import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { productionGame } from "./fixtures";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

// On a phone a tap opens the card as a sheet at the bottom of the screen.
test("unit info: production builder, phone", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame());
  await page.getByTestId("production-builder-drawer").waitFor();
  await page.getByTestId("unit-info-carrier").tap();
  await page.getByTestId("unit-info-carrier-card").waitFor();
  await shot(page, testInfo, "3-production-phone");
});
