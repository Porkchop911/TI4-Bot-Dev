import { actor } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import type { MockGameOptions } from "../_shared/mockGame";

const option = (unit: string, label: string, cost: number) => ({
  id: `build|${unit}`,
  kind: "produce",
  label,
  payload: { unit, cost, count: 1, available_resources: 9 },
});

/** The production builder for Sol (the viewing seat): the units a space dock offers, the mech and a unit the content does not know. */
export const productionChoice = (extra: ReturnType<typeof option>[] = []) => ({
  prompt: "Produce units in system 18",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ kind: "production_capacity", amount: 5, paid: 0 }],
  },
  options: [
    option("dreadnought", "Produce dreadnought", 4),
    option("carrier", "Produce carrier", 3),
    option("flagship", "Produce flagship", 8),
    option("mech", "Produce mech", 2),
    option("infantry", "Produce infantry", 1),
    ...extra,
    { id: "done_producing", kind: "decline", label: "Done producing", payload: {} },
  ],
});

/** Sol researched Dreadnought II and Advanced Carrier II; its leaders as the server reports them. */
export const solSeat = () =>
  playerWithHand({
    technologies: ["dn2", "ac2"],
    leaders: { solagent: "Readied", solcommander: "Locked", solhero: "Locked" },
  });

export const productionGame = (extra: ReturnType<typeof option>[] = []): MockGameOptions => ({
  players: [solSeat(), opponent],
  choice: productionChoice(extra),
});

export const unknownUnit = option("mystery_unit", "Produce mystery unit", 1);
export { actor, opponent, playerWithHand };
