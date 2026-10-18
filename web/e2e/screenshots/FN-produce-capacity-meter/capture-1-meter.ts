import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";

// The produce dialog keeps a live capacity meter for the system it builds in. The engine sends the
// present position (`details.capacity`) and, per build option, what the build gives and takes
// (`capacity_provided`, `capacity_needed`, `capacity_role`, `space_placement`); the dialog adds up
// the staged builds. Over capacity is orange and never blocks the build.
const SIZES = {
  desktop: { width: 1440, height: 1000 },
  phone: { width: 390, height: 844 },
} as const;

const option = (
  unit: string,
  cost: number,
  extra: { provided?: number; needed?: number; role: string; placement: string; count?: number; fleetShips?: number },
) => {
  const count = extra.count ?? 1;
  return {
    id: `build|${unit}|${count}`,
    kind: "produce",
    label: `produce ${count}x ${unit} for ${cost}`,
    payload: {
      unit,
      count,
      cost,
      printed_cost: cost,
      production_spent: 1,
      available_resources: 20,
      fleet_ships: extra.fleetShips ?? 0,
      capacity_provided: extra.provided ?? 0,
      capacity_needed: extra.needed ?? 0,
      capacity_role: extra.role,
      space_placement: extra.placement,
    },
  };
};

const production = (capacity: Record<string, unknown>, infantryPlacement: "never" | "always") => ({
  prompt: "produce in 18 (6 left)",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ kind: "production_capacity", amount: 6, paid: 0 }],
  },
  details: { fleet_supply: { used: 1, limit: 5 }, capacity },
  options: [
    option("carrier", 3, { provided: 6, role: "none", placement: "always", fleetShips: 1 }),
    option("fighter", 1, { needed: 2, role: "fighter", placement: "always", count: 2 }),
    option("infantry", 1, { needed: 2, role: "ground", placement: infantryPlacement, count: 2 }),
    { id: "done_producing", kind: "decline", label: "produce nothing further" },
  ],
});

const FIGHTER = "build|fighter|2";
const CARRIER = "build|carrier|1";
const INFANTRY = "build|infantry|2";

interface Scenario {
  name: string;
  capacity: Record<string, unknown>;
  infantry?: "never" | "always";
  stage: [string, number][];
  text: string;
  over: boolean;
}

const scenarios: Scenario[] = [
  {
    name: "1-no-capacity-yet",
    capacity: { transport: 0, consumed: 0, fighters: 0, unused_support: 0 },
    stage: [],
    text: "Capacity 0 / 0",
    over: false,
  },
  {
    name: "2-carrier-raises-capacity",
    capacity: { transport: 0, consumed: 0, fighters: 0, unused_support: 0 },
    stage: [[CARRIER, 1], [FIGHTER, 2]],
    text: "Capacity 4 / 6",
    over: false,
  },
  {
    name: "3-fighters-over-capacity",
    capacity: { transport: 4, consumed: 2, fighters: 2, unused_support: 0 },
    stage: [[FIGHTER, 3]],
    text: "Capacity 8 / 4 (4 fighters over: they would be removed)",
    over: true,
  },
  {
    name: "4-arborec-planetless-system",
    capacity: { transport: 2, consumed: 0, fighters: 0, unused_support: 0 },
    infantry: "always",
    stage: [[INFANTRY, 1], [FIGHTER, 1]],
    text: "Capacity 4 / 2 (2 units over: they would be removed)",
    over: true,
  },
];

for (const [sizeName, size] of Object.entries(SIZES)) {
  for (const s of scenarios) {
    test(`${s.name} ${sizeName}`, async ({ page }, testInfo) => {
      await page.setViewportSize(size);
      await openMockedGame(page, { choice: production(s.capacity, s.infantry ?? "never") });
      const counter = page.getByTestId("capacity-counter");
      await counter.waitFor();
      for (const [id, times] of s.stage) {
        for (let i = 0; i < times; i += 1) await page.getByTestId(`produce-unit-btn-${id}`).click();
      }
      await expect(counter).toHaveText(s.text);
      const build = page.getByRole("button", { name: /Confirm builds|^Build/ });
      if (s.over) await expect(build).toHaveAttribute("data-over-capacity", "true");
      else await expect(build).not.toHaveAttribute("data-over-capacity", "true");
      await counter.scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await shot(page, testInfo, `${s.name}-${sizeName}`, { of: page.getByTestId("production-builder-drawer"), pad: 4 });
    });
  }
}
