import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openLeadership } from "./leadership";

// The panel after buying one token: Auto-pay planned Lodor, and a "Pay on the map" button appears.
test("leadership map payment: panel with Pay on the map", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await expect(page.getByTestId("token-pay-on-map")).toBeVisible();
  await shot(page, testInfo, "1-panel-pay-on-map", { of: page.getByTestId("command-token-panel"), pad: 8 });
});
