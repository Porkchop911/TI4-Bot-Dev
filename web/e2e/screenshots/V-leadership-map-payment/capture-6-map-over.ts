import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// Lodor plus Jord (5 against 3): more than the bill needs, so the engine would not take it.
test("leadership map payment: over-covered", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await payOnMap(page);
  await clickPlanets(page, "jord");
  await expect(page.getByTestId("token-bar-problem")).toContainText("More than needed");
  await expect(page.getByTestId("token-bar-confirm")).toBeDisabled();
  await shot(page, testInfo, "6-map-over-covered");
});
