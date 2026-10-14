import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// Jord and Quann (2 + 1) pay the 3 exactly instead of Auto-pay's Lodor: covered, Confirm enabled.
test("leadership map payment: exactly covered", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await payOnMap(page);
  await clickPlanets(page, "lodor", "jord", "quann");
  await expect(page.getByTestId("token-bar-remaining")).toContainText("Covered");
  await expect(page.getByTestId("token-bar-confirm")).toBeEnabled();
  await shot(page, testInfo, "5-map-exact");
});
