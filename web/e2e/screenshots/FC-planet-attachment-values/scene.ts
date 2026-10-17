import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { actor, agendaCard, galleryBoard } from "../_shared/fixtures";

/**
 * Three boosted planets on the gallery board, all controlled by the viewer:
 *  - Lodor (#26, printed 3R/1I) with Mining World: +2 resources -> 5R/1I
 *  - Quann (#25, printed 2R/1I) with Dyson Sphere: +2 resources, +1 influence -> 4R/2I (both kinds)
 *  - Jord (#18, printed 2R/2I) under the Core Mining law: +2 resources -> 4R/2I (a law, not a card on the planet)
 * The values the engine pays with are the boosted ones, so the payment options carry them.
 */
const MINING = { source: "Mining World", resources: 2, influence: 0 };
const DYSON = { source: "Dyson Sphere", resources: 2, influence: 1 };
const CORE = { source: "Core Mining", resources: 2, influence: 0 };

const sys = galleryBoard.systems;
export const boostedBoard = {
  ...galleryBoard,
  systems: {
    ...sys,
    "18": {
      ...sys["18"],
      planets: { ...sys["18"].planets, jord: { ...sys["18"].planets.jord, value_boosts: [CORE] } },
    },
    "26": {
      ...sys["26"],
      planets: {
        lodor: { ...sys["26"].planets.lodor, controlled_by: actor, attachments: ["miningworld"], value_boosts: [MINING] },
      },
    },
    "25": {
      ...sys["25"],
      planets: {
        quann: { ...sys["25"].planets.quann, controlled_by: actor, attachments: ["dysonsphere"], value_boosts: [DYSON] },
      },
    },
  },
};

export const payChoice = {
  prompt: "pay 8 more resources to produce",
  context: { subtype: "pay_resources", outstanding: [{ kind: "resources", amount: 8, paid: 0 }] },
  options: [
    { id: "exhaust|jord", label: "Exhaust Jord", kind: "pay", payload: { worth: 4, owed: 8, kind: "resources", planet: "jord", planet_name: "Jord" } },
    { id: "exhaust|lodor", label: "Exhaust Lodor", kind: "pay", payload: { worth: 5, owed: 8, kind: "resources", planet: "lodor", planet_name: "Lodor" } },
    { id: "exhaust|quann", label: "Exhaust Quann", kind: "pay", payload: { worth: 4, owed: 8, kind: "resources", planet: "quann", planet_name: "Quann" } },
    { id: "trade_good", label: "Spend one trade good", kind: "pay", payload: { worth: 1 } },
    { id: "decline", label: "Done", kind: "decline" },
  ],
};

export const voteChoice = {
  prompt: "exhaust a planet to vote FOR",
  context: { subtype: "vote_exhaust_planet", details: { agenda_card: agendaCard } },
  options: [
    { id: "lodor", label: "exhaust Lodor for 1 vote", kind: "vote_planet", payload: { votes: 1, planet_name: "Lodor" } },
    { id: "quann", label: "exhaust Quann for 2 votes", kind: "vote_planet", payload: { votes: 2, planet_name: "Quann" } },
    { id: "jord", label: "exhaust Jord for 2 votes", kind: "vote_planet", payload: { votes: 2, planet_name: "Jord" } },
    { id: "decline", label: "Done", kind: "decline" },
  ],
};

export const SIZES = [
  { label: "desktop", width: 1440, height: 900 },
  { label: "phone", width: 390, height: 844 },
] as const;

export async function open(page: Page, extra: Parameters<typeof openMockedGame>[1] = {}) {
  await openMockedGame(page, { players: [playerWithHand(), opponent], board: boostedBoard as never, choice: null, ...extra });
}
