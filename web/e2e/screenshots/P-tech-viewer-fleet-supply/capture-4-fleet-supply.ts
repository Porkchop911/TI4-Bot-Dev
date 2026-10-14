import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";

const production = (details: Record<string, unknown> | undefined) => ({
  prompt: "produce in 18 (3 left)",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ kind: "production_capacity", amount: 3, paid: 0 }],
  },
  options: [
    { id: "build|carrier|1", kind: "produce", label: "produce 1x carrier for 3", payload: { unit: "carrier", count: 1, cost: 3, available_resources: 5 } },
    { id: "done_producing", kind: "decline", label: "produce nothing further" },
  ],
  ...(details ? { details } : {}),
});

// The server now sends `details.fleet_supply` on the produce decision (ships charged vs. pool limit in that system).
test("production: fleet supply from the server", async ({ page }, testInfo) => {
  await openMockedGame(page, { choice: production({ fleet_supply: { used: 2, limit: 4 } }) });
  await page.getByTestId("fleet-supply-counter").waitFor();
  await shot(page, testInfo, "4-fleet-supply", { of: page.getByTestId("production-builder-drawer"), pad: 4 });
});

// Letnev's hero round: no pool limit.
test("production: unlimited fleet supply", async ({ page }, testInfo) => {
  await openMockedGame(page, { choice: production({ fleet_supply: { used: 6, limit: 10000, unlimited: true } }) });
  await page.getByTestId("fleet-supply-counter").waitFor();
  await shot(page, testInfo, "5-fleet-supply-unlimited", { of: page.getByTestId("production-builder-drawer"), pad: 4 });
});
