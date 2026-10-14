import { expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView, PlacedUnitView, ReachOrigin } from "../../../src/protocol/types";

export const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;
export const RIVAL = galleryPlayers[1].id;
export const THIRD = "third_seat";
export const players = [...galleryPlayers, { ...galleryPlayers[1], id: THIRD, faction: "letnev" }];

const planet = (id: string, label: string) => ({ id, label, resources: 2, influence: 1, traits: [], tech_specialties: [] });
const unit = (owner: string, unit_type: string, extra: Partial<PlacedUnitView> = {}): PlacedUnitView => ({
  owner,
  unit_type,
  damaged: false,
  ...extra,
});

// Mecatol Rex (#18) in the middle holds units of three players, so the overview's units section has
// to keep them apart. Ships stand in the ring around it.
export const board: BoardView = {
  map_tiles: [
    { system_id: "18", label: "Mecatol Rex", q: 0, r: 0, planets: [planet("mr", "Mecatol Rex")] },
    { system_id: "01", label: "Jord - Sol", q: 0, r: 1, planets: [planet("jord", "Jord")] },
    { system_id: "27", label: "Lodor", q: 1, r: 0, planets: [planet("lodor", "Lodor")] },
    { system_id: "41", label: "Gravity Rift", q: 1, r: -1, anomalies: ["gravity rift"], planets: [] },
    { system_id: "26", label: "Lodor II", q: 0, r: -1, planets: [planet("l2", "Lodor II")] },
    { system_id: "25", label: "Quann", q: -1, r: 0, planets: [planet("quann", "Quann")] },
    { system_id: "30", label: "Empty system", q: -1, r: 1, planets: [] },
  ],
  systems: {
    "18": {
      system_id: "18",
      command_tokens: [],
      planets: {},
      units: [
        unit(RIVAL, "cruiser"),
        unit(actor, "dreadnought", { damaged: true }),
        unit(THIRD, "destroyer"),
        unit(actor, "cruiser"),
        unit(THIRD, "infantry", { planet: "mr" }),
        unit(actor, "infantry", { planet: "mr" }),
        unit(actor, "infantry", { planet: "mr" }),
        unit(RIVAL, "pds", { planet: "mr" }),
        unit(RIVAL, "infantry", { planet: "mr" }),
      ],
    },
    "01": { system_id: "01", command_tokens: [], planets: {}, units: [unit(actor, "carrier"), unit(actor, "destroyer"), unit(actor, "destroyer")] },
    "27": { system_id: "27", command_tokens: [], planets: {}, units: [unit(RIVAL, "dreadnought"), unit(RIVAL, "cruiser")] },
    "41": { system_id: "41", command_tokens: [], planets: {}, units: [unit(THIRD, "carrier")] },
    "26": { system_id: "26", command_tokens: [], planets: {}, units: [] },
    "25": { system_id: "25", command_tokens: [], planets: {}, units: [] },
    "30": { system_id: "30", command_tokens: [], planets: {}, units: [] },
  },
};

const ship = (type_id: string, count: number, gravity_drive = 0) => ({ type_id, count, damaged: 0, gravity_drive, ionian: 0 });
const mine: ReachOrigin = { owner: actor, origin: "01", ships: [ship("carrier", 1), ship("destroyer", 2)] };
const theirs: ReachOrigin[] = [
  { owner: RIVAL, origin: "27", ships: [ship("dreadnought", 1), ship("cruiser", 1, 1)] },
  { owner: THIRD, origin: "41", ships: [ship("carrier", 1)] },
];
export const ANSWERS = {
  own: [mine],
  enemy: theirs,
  mixed: [mine, ...theirs],
  empty: [],
} as const;
export type Scenario = keyof typeof ANSWERS;

/** Opens the game with a mocked engine that answers "who can go here" with the scenario's ships. */
export async function open(page: Page, size: { width: number; height: number }, scenario: Scenario) {
  await page.setViewportSize(size);
  const asked: string[] = [];
  await openMockedGame(page, {
    board,
    players,
    nicknames: { [actor]: "You", [RIVAL]: "Hacan Fox", [THIRD]: "Letnev Lee" },
    onClientMessage: (message, reply) => {
      if (message.type !== "query_reach") return;
      asked.push(message.system_id);
      reply({
        type: "reach_into",
        protocol_version: message.protocol_version,
        game_id: message.game_id,
        request_id: message.request_id,
        system_id: message.system_id,
        as_of_version: 1,
        origins: ANSWERS[scenario],
      });
    },
  });
  await page.getByTestId("system-hex-18").click();
  await expect(page.getByTestId("system-inspector")).toBeVisible();
  return asked;
}
