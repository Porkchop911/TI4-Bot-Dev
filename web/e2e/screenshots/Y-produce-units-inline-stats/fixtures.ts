import { actor } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import type { MockGameOptions } from "../_shared/mockGame";

interface Offer {
  id: string;
  kind: string;
  label: string;
  payload: Record<string, unknown>;
}

const option = (unit: string, cost: number, count = 1, resources = 9): Offer => ({
  id: `build|${unit}|${count}`,
  kind: "produce",
  label: `produce ${count}x ${unit} for ${cost}`,
  payload: { unit, cost, count, production_spent: count, available_resources: resources },
});

/** The units a space dock offers (flagship and mech included); with 9 resources the war sun is out of reach. */
export const productionChoice = (resources = 9, capacity = 6) => ({
  prompt: "Produce units in system 18",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ kind: "production_capacity", amount: capacity, paid: 0 }],
  },
  options: [
    option("fighter", 1, 2, resources),
    option("destroyer", 1, 1, resources),
    option("cruiser", 2, 1, resources),
    option("carrier", 3, 1, resources),
    option("dreadnought", 4, 1, resources),
    option("flagship", 8, 1, resources),
    option("warsun", 12, 1, resources),
    option("infantry", 1, 2, resources),
    option("mech", 2, 1, resources),
    { id: "done_producing", kind: "decline", label: "Done producing", payload: {} },
  ],
});

type Faction = "sol" | "hacan";

export const productionGame = (
  faction: Faction,
  technologies: string[],
  resources = 9,
): MockGameOptions => ({
  players: [playerWithHand({ faction, technologies }), opponent],
  choice: productionChoice(resources),
});

export { actor };
