import type { ChoiceOption } from "../../../src/protocol/types";

// Letnev attacker, three cruisers hitting on 7: faces 3, 9, 1 (one hit, two misses), 5 trade goods.
export const DICE = [
  { unit: "cruiser", unit_index: 0, die: 0, face: 3, bonus: 0, hits_on: 7, rerolled: false },
  { unit: "cruiser", unit_index: 0, die: 1, face: 9, bonus: 0, hits_on: 7, rerolled: false },
  { unit: "cruiser", unit_index: 0, die: 2, face: 1, bonus: 0, hits_on: 7, rerolled: false },
];

const option = (id: string, label: string, kind: string): ChoiceOption =>
  ({ id, label, kind, payload: {} }) as unknown as ChoiceOption;

const ABILITY = {
  name: "Munitions Reserves",
  window: "At the start of each round of space combat",
  effect: "You may spend 2 trade goods to re-roll any number of your dice during that combat round.",
};

/** The Munitions Reserves question, asked once the fleet has rolled. */
export const afterRollOffer = (dice: typeof DICE = DICE, misses = 2) => ({
  prompt: "spend 2 trade goods for Munitions Reserves",
  context: { subtype: "munitions_reserves_reroll", source: { FactionAbility: "munitions" } },
  options: [
    option("munitions", `reroll all ${misses} missed dice`, "ability"),
    ...(misses > 1 ? [option("munitions_choose", "choose which dice to reroll", "ability")] : []),
    option("decline", "decline", "decline"),
  ],
  details: { kind: "ability_offer", ability: ABILITY, cost: { trade_goods: 2, have: 5 }, dice, misses },
});

/** The per-die question after "choose which missed dice": the first or the second miss. */
export const perDieQuestion = (current: number) => ({
  prompt: `reroll die ${current + 1} of cruiser`,
  context: { subtype: "munitions_reroll_die", source: { FactionAbility: "munitions" } },
  options: [
    option(`reroll|0:${current}`, `reroll die ${current + 1} of cruiser (shows ${DICE[current].face})`, "reroll_die"),
    option("decline", "decline", "decline"),
  ],
  details: { dice: DICE, current },
});
