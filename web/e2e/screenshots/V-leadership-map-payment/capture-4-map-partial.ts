import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// Jord staged (2 of 3): one influence remaining, Confirm disabled.
test("leadership map payment: partially covered", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await payOnMap(page);
  await clickPlanets(page, "lodor", "jord");
  await expect(page.getByTestId("token-bar-remaining")).toContainText("1 remaining");
  await expect(page.getByTestId("token-bar-confirm")).toBeDisabled();
  await shot(page, testInfo, "4-map-partial");
});
