import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";

// The produce-units drawer previews the fleet supply: `used +X / limit` for the X ships the staged
// builds add (the engine names them per option as `fleet_ships`), and the Build button turns
// alert orange when used + X would pass the limit. Letnev's limit already includes Armada.
const SIZES = {
  desktop: { width: 1440, height: 1000 },
  phone: { width: 390, height: 1500 },
} as const;

const option = (unit: string, cost: number, fleetShips: number, count = 1) => ({
  id: `build|${unit}|${count}`,
  kind: "produce",
  label: `produce ${count}x ${unit} for ${cost}`,
  payload: { unit, count, cost, printed_cost: cost, production_spent: 1, available_resources: 20, fleet_ships: fleetShips },
});

const production = (fleetSupply: Record<string, unknown>) => ({
  prompt: "produce in 18 (6 left)",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ kind: "production_capacity", amount: 6, paid: 0 }],
  },
  details: { fleet_supply: fleetSupply },
  options: [
    option("cruiser", 2, 1),
    option("destroyer", 1, 1),
    option("fighter", 1, 0, 2),
    { id: "done_producing", kind: "decline", label: "produce nothing further" },
  ],
});

const scenarios: { name: string; supply: Record<string, unknown>; stage: number; text: string; over: boolean }[] = [
  { name: "1-nothing-staged", supply: { used: 2, limit: 5 }, stage: 0, text: "2 / 5", over: false },
  { name: "2-within-limit", supply: { used: 2, limit: 5 }, stage: 2, text: "2 +2 / 5", over: false },
  { name: "3-at-limit", supply: { used: 2, limit: 5 }, stage: 3, text: "2 +3 / 5", over: false },
  { name: "4-over-limit", supply: { used: 2, limit: 5 }, stage: 4, text: "2 +4 / 5", over: true },
  { name: "5-letnev-armada", supply: { used: 2, limit: 5, fleet_supply_bonus: 2 }, stage: 3, text: "2 +3 / 5", over: false },
  { name: "6-letnev-armada-over", supply: { used: 2, limit: 5, fleet_supply_bonus: 2 }, stage: 4, text: "2 +4 / 5", over: true },
];

for (const [sizeName, size] of Object.entries(SIZES)) {
  for (const s of scenarios) {
    test(`${s.name} ${sizeName}`, async ({ page }, testInfo) => {
      await page.setViewportSize(size);
      await openMockedGame(page, { choice: production(s.supply) });
      const counter = page.getByTestId("fleet-supply-counter");
      await counter.waitFor();
      for (let i = 0; i < s.stage; i += 1) await page.getByTestId("produce-unit-btn-build|cruiser|1").click();
      await expect(counter).toHaveText(s.text);
      const build = page.getByRole("button", { name: "Confirm builds" });
      if (s.over) await expect(build).toHaveAttribute("data-over-fleet-supply", "true");
      else await expect(build).not.toHaveAttribute("data-over-fleet-supply", "true");
      if (s.supply.armada) await expect(page.getByTestId("production-armada")).toBeVisible();
      await page.waitForTimeout(300);
      await shot(page, testInfo, `${s.name}-${sizeName}`, { of: page.getByTestId("production-builder-drawer"), pad: 4 });
    });
  }
}
