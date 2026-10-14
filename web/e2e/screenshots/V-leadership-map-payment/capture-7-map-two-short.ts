import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// Two tokens bought (6 influence, all of it): Quann clicked off leaves Lodor and Jord, 5 of 6.
test("leadership map payment: insufficient influence for two tokens", async ({ page }, testInfo) => {
  await openLeadership(page, 2);
  await payOnMap(page);
  await clickPlanets(page, "quann");
  await expect(page.getByTestId("token-bar-problem")).toContainText("Short by 1");
  await expect(page.getByTestId("token-bar-confirm")).toBeDisabled();
  await shot(page, testInfo, "7-map-two-tokens-short");
});
