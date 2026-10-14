import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { clickPlanets, openLeadership, payOnMap } from "./leadership";

// After clicking on the map, reopening the panel: its planet list shows the same selection.
test("leadership map payment: panel list in sync", async ({ page }, testInfo) => {
  await openLeadership(page, 1);
  await payOnMap(page);
  await clickPlanets(page, "lodor", "jord", "quann");
  await page.getByTestId("resume-choice-button").click();
  const panel = page.getByTestId("command-token-panel");
  await panel.waitFor();
  await expect(page.getByTestId("token-payment-planet-jord")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("token-payment-planet-lodor")).toHaveAttribute("aria-pressed", "false");
  await shot(page, testInfo, "8-panel-synced", { of: panel, pad: 8 });
});
