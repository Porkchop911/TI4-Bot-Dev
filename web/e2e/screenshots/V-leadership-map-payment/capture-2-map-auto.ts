import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openLeadership, payOnMap } from "./leadership";

// Panel minimised: the planets that can pay are ringed with their influence; Auto-pay's pick is staged.
test("leadership map payment: Auto-pay staged on the map", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await payOnMap(page);
  await expect(page.getByTestId("token-bar-remaining")).toContainText("Covered");
  await shot(page, testInfo, "2-map-auto-pay");
});
