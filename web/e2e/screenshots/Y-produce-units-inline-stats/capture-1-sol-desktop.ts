import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { productionGame } from "./fixtures";

// Sol with Dreadnought II and Advanced Carrier II: every option shows its numbers without hovering.
test("produce units: Sol with upgrades, desktop", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame("sol", ["dn2", "ac2"]));
  await page.getByTestId("production-builder-drawer").waitFor();
  await shot(page, testInfo, "1-sol-desktop");
});
