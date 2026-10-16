import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { productionGame } from "../Y-produce-units-inline-stats/fixtures";
import { VIEWPORT, shotAndCheck } from "./_layer";

test("dialog header layer: production drawer", async ({ page }, testInfo) => {
  await page.setViewportSize(VIEWPORT);
  await openMockedGame(page, productionGame("sol", ["dn2", "ac2"]));
  await shotAndCheck(page, testInfo, "1-production", page.getByTestId("production-builder-drawer"));
});
