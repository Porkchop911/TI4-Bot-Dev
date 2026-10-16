import type { MockChoice } from "../_shared/mockGame";
import { opponent } from "../_shared/players";

export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const build = (unit: string, cost: number) => ({
  id: `build|${unit}|1`,
  kind: "produce",
  label: `produce 1x ${unit} for ${cost}`,
  payload: { unit, count: 1, cost, printed_cost: cost, discount: 0, production_spent: 1, available_resources: 6 },
});

/** What the engine offers the Cabal (Amalgamation) holding a captured carrier. */
const exchange = (unit: string, printed: number) => ({
  id: `exchange|${unit}`,
  kind: "produce",
  label: `produce 1x ${unit} by returning a captured ${unit}`,
  payload: {
    unit,
    count: 1,
    placed: 1,
    cost: 0,
    printed_cost: printed,
    discount: 0,
    production_spent: 1,
    available_resources: 6,
    exchange: true,
  },
});

export const cabalProduction: MockChoice = {
  prompt: "produce in 14 (3 left)",
  context: {
    subtype: "produce_unit",
    target: { System: "14" },
    outstanding: [{ kind: "production_capacity", amount: 3, paid: 0 }],
  },
  options: [
    build("fighter", 1),
    build("carrier", 3),
    exchange("carrier", 3),
    build("cruiser", 2),
    build("dreadnought", 4),
    build("infantry", 1),
    { id: "done_producing", kind: "decline", label: "produce nothing further", payload: {} },
  ],
};

const RAW = "player_5b96c0ffee0123456789abcdef0123456789abcdef0123456789abcdef012345";

/** The bombardment question whose option labels carry the raw seat id (engine: `{player}'s units`). */
export const bombardmentTarget: MockChoice = {
  prompt: "whose units on mecatol_rex take the bombardment's next hits (2 hits)",
  context: { subtype: "bombardment_target", target: { System: "18" } },
  options: [
    {
      id: opponent.id,
      kind: "bombardment_target",
      label: `${opponent.id}'s units`,
      payload: { system: "18", planet: "mecatol_rex" },
    },
    {
      id: RAW,
      kind: "bombardment_target",
      label: `${RAW}'s units`,
      payload: { system: "18", planet: "mecatol_rex" },
    },
  ],
};
