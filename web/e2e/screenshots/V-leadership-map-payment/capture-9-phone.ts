import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// Phone width: the same exactly-covered state.
test("leadership map payment: phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openLeadership(page, 1);
  await payOnMap(page);
  await clickPlanets(page, "lodor", "jord", "quann");
  await expect(page.getByTestId("token-bar-remaining")).toContainText("Covered");
  await shot(page, testInfo, "9-phone-exact");
});
