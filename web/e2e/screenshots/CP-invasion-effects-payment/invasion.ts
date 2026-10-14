import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { actor, galleryBoard } from "../_shared/fixtures";
import type { BoardView } from "../../../src/protocol/types";

/** Planets the invader controls and may exhaust: id -> influence (printed on the gallery tiles). */
export const payable: Record<string, { label: string; influence: number }> = {
  wellon: { label: "Wellon", influence: 2 },
  tarmann: { label: "Tar'Mann", influence: 1 },
  torkan: { label: "Torkan", influence: 3 },
  centauri: { label: "Centauri", influence: 3 },
  lodor: { label: "Lodor", influence: 1 },
};

/**
 * The gallery board with Mecatol Rex (1/6) in system 18, the invader's ships and infantry in its
 * space area, an invasion under way there, and the payable planets controlled by the invader.
 */
export function invasionBoard(phase: string): BoardView {
  const systems = Object.fromEntries(
    Object.entries(galleryBoard.systems).map(([id, system]) => [
      id,
      {
        ...system,
        planets: Object.fromEntries(
          Object.entries(system.planets).map(([planet, view]) => [
            planet,
            planet in payable ? { ...view, controlled_by: actor, exhausted: false } : view,
          ]),
        ),
      },
    ]),
  );
  return {
    ...galleryBoard,
    map_tiles: galleryBoard.map_tiles?.map((tile) =>
      tile.system_id === "18"
        ? { ...tile, planets: [{ id: "mr", label: "Mecatol Rex", resources: 1, influence: 6, legendary: false }] }
        : tile,
    ),
    systems: {
      ...systems,
      "18": {
        system_id: "18",
        command_tokens: [actor],
        planets: { mr: { planet_id: "mr", controlled_by: null, exhausted: false } },
        units: [
          { unit_type: "dreadnought", owner: actor, damaged: false },
          { unit_type: "carrier", owner: actor, damaged: false },
          { unit_type: "infantry", owner: actor, damaged: false },
          { unit_type: "infantry", owner: actor, damaged: false },
        ],
      },
    },
    invasion: {
      system_id: "18",
      invasion_seq: 1,
      invader: actor,
      phase,
      planets: ["mr"],
      current_planet: "mr",
      defender: null,
      ground_round: 0,
      odds_context: {},
    },
  } as BoardView;
}

/** The engine's custodians offer (27.2), asked before any ground force lands. */
export const custodiansOffer = {
  prompt: "spend 6 influence to remove the custodians token",
  context: { subtype: "remove_custodians", target: { System: "18" }, invasion_seq: 1 },
  options: [
    { id: "no", kind: "decline", label: "leave it" },
    { id: "yes", kind: "custodians", label: "remove it for a victory point" },
  ],
};

/** The payment question that follows "yes": six influence, one option per payable planet. */
export const custodiansPayment = {
  prompt: "pay 6 more influence",
  context: {
    subtype: "pay_influence",
    invasion_seq: 1,
    outstanding: [{ kind: "Influence", amount: 6, paid: 0 }],
  },
  options: [
    ...Object.entries(payable).map(([id, { label, influence }]) => ({
      id: `exhaust|${id}`,
      kind: "pay",
      label: `exhaust ${id} for ${influence} influence`,
      payload: { worth: influence, owed: 6, kind: "influence", source: "influence", planet: id, planet_name: label },
    })),
    { id: "trade_good", kind: "pay", label: "spend a trade good", payload: { worth: 1, owed: 6, kind: "influence", source: "trade_good" } },
  ],
};

/** Freelancers, explored on a planet just taken: "You may produce 1 unit in this system." */
export const freelancersBuild = {
  prompt: "Freelancers: produce one unit in 18",
  context: { subtype: "produce_one_unit", optional: true, target: { System: "18" }, invasion_seq: 1 },
  options: [
    ...[
      ["fighter", 1],
      ["destroyer", 1],
      ["cruiser", 2],
      ["carrier", 3],
      ["dreadnought", 4],
    ].map(([unit, cost]) => ({
      id: `build|${unit}|1`,
      kind: "produce",
      label: `produce 1x ${unit} for ${cost}`,
      payload: { unit, count: 1, cost, system: "18" },
    })),
    { id: "decline", kind: "decline", label: "Decline" },
  ],
};

export async function openInvasionChoice(
  page: Page,
  phase: string,
  choice: Record<string, unknown>,
  onClientMessage?: (message: unknown) => void,
) {
  await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 1 }), opponent],
    board: invasionBoard(phase),
    choice: choice as never,
    onClientMessage: onClientMessage ? (message) => onClientMessage(message) : undefined,
  });
}
