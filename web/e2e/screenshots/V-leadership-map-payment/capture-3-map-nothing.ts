import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// Auto-pay's planet clicked off: nothing is staged, the whole bill is still to pay.
test("leadership map payment: nothing selected", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await payOnMap(page);
  await clickPlanets(page, "lodor");
  await expect(page.getByTestId("token-bar-remaining")).toContainText("3 remaining");
  await expect(page.getByTestId("token-bar-confirm")).toBeDisabled();
  await shot(page, testInfo, "3-map-nothing-selected");
});
