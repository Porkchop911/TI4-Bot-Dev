import type {
  BoardTileView,
  BoardView,
  LobbyDto,
  PlayerView,
  SystemView,
} from "../protocol/types.ts";
import { actor } from "./decisionGalleryCases.ts";

export const gallerySeating = [actor, "other_seat"];
export const galleryLobby: LobbyDto = {
  game_id: "gallery-placeholder",
  phase: "running",
  lobby_version: 1,
  host_player_id: actor,
  slots: gallerySeating.map((id, position) => ({
    slot_id: `gallery-${position}`,
    position,
    occupant: id,
    nickname: position === 0 ? "Alex (Sol)" : "Blair (Hacan)",
    ready: true,
    connected: true,
    can_take_over: false,
  })),
};

const player = (id: string, faction: string, trade_goods: number): PlayerView => ({
  id,
  faction,
  victory_points: 2,
  trade_goods,
  commodities: 0,
  tactic_tokens: 3,
  fleet_tokens: 3,
  strategic_tokens: 2,
  passed: false,
  strategy_cards: [],
  exhausted_strategy_cards: [],
  technologies: [],
  exhausted_technologies: [],
  relics: [],
  exhausted_relics: [],
  action_cards_count: 0,
  secret_objectives_count: 0,
  leaders: {},
});

export const galleryPlayers = [player(actor, "sol", 2), player("other_seat", "hacan", 1)];

const mapTiles: BoardTileView[] = [
  // Ring 0 (Center)
  {
    system_id: "18",
    label: "Mecatol Rex",
    q: 0,
    r: 0,
    planets: [
      { id: "jord", label: "Jord", resources: 2, influence: 2 },
      { id: "exhausted", label: "Exhausted World", resources: 1, influence: 3 },
    ],
  },

  // Ring 1 (6 systems)
  {
    system_id: "26",
    label: "Lodor",
    q: 1,
    r: 0,
    wormholes: ["alpha"],
    planets: [{ id: "lodor", label: "Lodor", resources: 3, influence: 1 }],
  },
  {
    system_id: "19",
    label: "Wellon",
    q: 1,
    r: -1,
    planets: [
      {
        id: "wellon",
        label: "Wellon",
        resources: 1,
        influence: 2,
        tech_specialties: ["cybernetic"],
      },
    ],
  },
  {
    system_id: "22",
    label: "Tar'Mann",
    q: 0,
    r: -1,
    planets: [
      {
        id: "tarmann",
        label: "Tar'Mann",
        resources: 1,
        influence: 1,
        tech_specialties: ["biotic"],
      },
    ],
  },
  {
    system_id: "25",
    label: "Quann",
    q: -1,
    r: 0,
    wormholes: ["beta"],
    planets: [{ id: "quann", label: "Quann", resources: 2, influence: 1 }],
  },
  {
    system_id: "24",
    label: "Mehar Xull",
    q: -1,
    r: 1,
    planets: [
      {
        id: "mehar_xull",
        label: "Mehar Xull",
        resources: 1,
        influence: 3,
        tech_specialties: ["warfare"],
      },
    ],
  },
  {
    system_id: "20",
    label: "Vefun 5",
    q: 0,
    r: 1,
    planets: [{ id: "vefun_5", label: "Vefun 5", resources: 2, influence: 2 }],
  },

  // Ring 2 (12 systems)
  {
    system_id: "27",
    label: "New Albion",
    q: 2,
    r: 0,
    planets: [
      {
        id: "new_albion",
        label: "New Albion",
        resources: 1,
        influence: 1,
        tech_specialties: ["biotic"],
      },
      { id: "starpoint", label: "Starpoint", resources: 3, influence: 1 },
    ],
  },
  {
    system_id: "41",
    label: "Gravity Rift",
    q: 1,
    r: 1,
    anomalies: ["gravity rift"],
    planets: [],
  },
  {
    system_id: "28",
    label: "Tequ'ran / Torkan",
    q: 0,
    r: 2,
    planets: [
      { id: "tequran", label: "Tequ'ran", resources: 2, influence: 0 },
      { id: "torkan", label: "Torkan", resources: 0, influence: 3 },
    ],
  },
  {
    system_id: "39",
    label: "Supernova",
    q: -1,
    r: 2,
    anomalies: ["supernova"],
    planets: [],
  },
  {
    system_id: "29",
    label: "Qucen'n / Rarron",
    q: -2,
    r: 2,
    planets: [
      { id: "qucenn", label: "Qucen'n", resources: 1, influence: 2 },
      { id: "rarron", label: "Rarron", resources: 0, influence: 3 },
    ],
  },
  {
    system_id: "42",
    label: "Nebula",
    q: -2,
    r: 1,
    anomalies: ["nebula"],
    planets: [],
  },
  {
    system_id: "30",
    label: "Centauri / Gral",
    q: -2,
    r: 0,
    planets: [
      { id: "centauri", label: "Centauri", resources: 1, influence: 3 },
      { id: "gral", label: "Gral", resources: 1, influence: 1, tech_specialties: ["propulsion"] },
    ],
  },
  {
    system_id: "40",
    label: "Asteroid Field",
    q: -1,
    r: -1,
    anomalies: ["asteroid field"],
    planets: [],
  },
  {
    system_id: "31",
    label: "Lazar / Sakulag",
    q: 0,
    r: -2,
    planets: [
      { id: "lazar", label: "Lazar", resources: 1, influence: 0, tech_specialties: ["cybernetic"] },
      { id: "sakulag", label: "Sakulag", resources: 2, influence: 1 },
    ],
  },
  {
    system_id: "32",
    label: "Dal Bootha / Xxehan",
    q: 1,
    r: -2,
    planets: [
      { id: "dal_bootha", label: "Dal Bootha", resources: 0, influence: 2 },
      { id: "xxehan", label: "Xxehan", resources: 1, influence: 1 },
    ],
  },
  {
    system_id: "45",
    label: "Cormund",
    q: 2,
    r: -2,
    anomalies: ["gravity rift"],
    planets: [],
  },
  {
    system_id: "33",
    label: "Corneeq / Resculon",
    q: 2,
    r: -1,
    planets: [
      { id: "corneeq", label: "Corneeq", resources: 1, influence: 2 },
      { id: "resculon", label: "Resculon", resources: 2, influence: 0 },
    ],
  },

  // Ring 3 (18 systems)
  {
    system_id: "1",
    label: "Sol Home",
    q: 3,
    r: 0,
    planets: [{ id: "sol_jord", label: "Jord", resources: 4, influence: 2 }],
  },
  {
    system_id: "34",
    label: "Abyz / Fria",
    q: 2,
    r: 1,
    planets: [
      { id: "abyz", label: "Abyz", resources: 3, influence: 0 },
      { id: "fria", label: "Fria", resources: 2, influence: 0 },
    ],
  },
  {
    system_id: "35",
    label: "Bereq / Sem-Lore",
    q: 1,
    r: 2,
    planets: [
      { id: "bereq", label: "Bereq", resources: 3, influence: 1 },
      { id: "sem_lore", label: "Sem-Lore", resources: 3, influence: 2 },
    ],
  },
  {
    system_id: "16",
    label: "Hacan Home",
    q: 0,
    r: 3,
    planets: [
      { id: "arretze", label: "Arretze", resources: 2, influence: 0 },
      { id: "hercan", label: "Hercan", resources: 1, influence: 1 },
      { id: "kamdorn", label: "Kamdorn", resources: 0, influence: 1 },
    ],
  },
  {
    system_id: "36",
    label: "Arinam / Meer",
    q: -1,
    r: 3,
    planets: [
      { id: "arinam", label: "Arinam", resources: 1, influence: 2 },
      { id: "meer", label: "Meer", resources: 0, influence: 4, tech_specialties: ["warfare"] },
    ],
  },
  {
    system_id: "37",
    label: "Arnor / Lor",
    q: -2,
    r: 3,
    planets: [
      { id: "arnor", label: "Arnor", resources: 2, influence: 1 },
      { id: "lor", label: "Lor", resources: 1, influence: 2 },
    ],
  },
  {
    system_id: "12",
    label: "Jol-Nar Home",
    q: -3,
    r: 3,
    planets: [
      { id: "jel_ser", label: "Jel-Ser", resources: 1, influence: 2 },
      { id: "nar", label: "Nar", resources: 2, influence: 3 },
    ],
  },
  {
    system_id: "38",
    label: "Bereg / Lirta IV",
    q: -3,
    r: 2,
    planets: [
      { id: "bereg", label: "Bereg", resources: 3, influence: 1 },
      { id: "lirta_iv", label: "Lirta IV", resources: 2, influence: 3 },
    ],
  },
  {
    system_id: "43",
    label: "Asteroid Field",
    q: -3,
    r: 1,
    anomalies: ["asteroid field"],
    planets: [],
  },
  {
    system_id: "13",
    label: "Sardakk Home",
    q: -3,
    r: 0,
    planets: [
      { id: "tren_lak", label: "Tren'Lak", resources: 1, influence: 0 },
      { id: "quinarre", label: "Quinarre", resources: 3, influence: 2 },
    ],
  },
  {
    system_id: "65",
    label: "Primor",
    q: -2,
    r: -1,
    planets: [{ id: "primor", label: "Primor", resources: 2, influence: 1, legendary: true }],
  },
  {
    system_id: "44",
    label: "Supernova",
    q: -1,
    r: -2,
    anomalies: ["supernova"],
    planets: [],
  },
  {
    system_id: "14",
    label: "Xxcha Home",
    q: 0,
    r: -3,
    planets: [
      { id: "archon_ren", label: "Archon Ren", resources: 2, influence: 3 },
      { id: "archon_tau", label: "Archon Tau", resources: 1, influence: 1 },
    ],
  },
  {
    system_id: "66",
    label: "Hope's End",
    q: 1,
    r: -3,
    planets: [
      { id: "hopes_end", label: "Hope's End", resources: 3, influence: 0, legendary: true },
    ],
  },
  {
    system_id: "46",
    label: "Nebula",
    q: 2,
    r: -3,
    anomalies: ["nebula"],
    planets: [],
  },
  {
    system_id: "10",
    label: "Letnev Home",
    q: 3,
    r: -3,
    planets: [
      { id: "arc_prime", label: "Arc Prime", resources: 4, influence: 0 },
      { id: "wren_terra", label: "Wren Terra", resources: 2, influence: 1 },
    ],
  },
  {
    system_id: "64",
    label: "Atlas",
    q: 3,
    r: -2,
    wormholes: ["beta"],
    planets: [{ id: "atlas", label: "Atlas", resources: 3, influence: 1 }],
  },
  {
    system_id: "79",
    label: "Mallice",
    q: 3,
    r: -1,
    wormholes: ["alpha", "beta"],
    planets: [{ id: "mallice", label: "Mallice", resources: 0, influence: 3, legendary: true }],
  },
];

const dynamicSystems: Record<string, SystemView> = {
  // Ring 0
  "18": {
    system_id: "18",
    command_tokens: [],
    planets: {
      jord: { planet_id: "jord", controlled_by: actor, exhausted: false },
      exhausted: { planet_id: "exhausted", controlled_by: actor, exhausted: true },
    },
    units: [
      { unit_type: "fighter", owner: actor, damaged: false },
      { unit_type: "fighter", owner: actor, damaged: false },
      { unit_type: "dreadnought", owner: actor, damaged: true },
      { unit_type: "dreadnought", owner: actor, damaged: false },
      { unit_type: "infantry", owner: actor, damaged: false },
      { unit_type: "infantry", owner: actor, planet: "jord", damaged: false },
    ],
  },

  // Ring 1
  "26": {
    system_id: "26",
    command_tokens: [],
    planets: {
      lodor: { planet_id: "lodor", controlled_by: null, exhausted: false },
    },
    units: [],
  },
  "19": {
    system_id: "19",
    // Pre-activated by Sol: blocked from activation!
    command_tokens: [actor],
    planets: {
      wellon: { planet_id: "wellon", controlled_by: actor, exhausted: false },
    },
    units: [{ unit_type: "cruiser", owner: actor, damaged: false }],
  },
  "22": {
    system_id: "22",
    // Pre-activated by Sol: blocked from activation!
    command_tokens: [actor],
    planets: {
      tarmann: { planet_id: "tarmann", controlled_by: actor, exhausted: false },
    },
    units: [{ unit_type: "infantry", owner: actor, planet: "tarmann", damaged: false }],
  },
  "25": {
    system_id: "25",
    command_tokens: [],
    planets: {
      quann: { planet_id: "quann", controlled_by: null, exhausted: false },
    },
    units: [],
  },
  "24": {
    system_id: "24",
    command_tokens: [],
    planets: {
      mehar_xull: { planet_id: "mehar_xull", controlled_by: null, exhausted: false },
    },
    units: [{ unit_type: "cruiser", owner: actor, damaged: false }],
  },
  "20": {
    system_id: "20",
    command_tokens: [],
    planets: {
      vefun_5: { planet_id: "vefun_5", controlled_by: null, exhausted: false },
    },
    units: [],
  },

  // Ring 2
  "27": {
    system_id: "27",
    command_tokens: [],
    planets: {
      new_albion: { planet_id: "new_albion", controlled_by: null, exhausted: false },
      starpoint: { planet_id: "starpoint", controlled_by: null, exhausted: false },
    },
    units: [],
  },
  "28": {
    system_id: "28",
    // Pre-activated by Hacan: Alex can still attack this!
    command_tokens: ["other_seat"],
    planets: {
      tequran: { planet_id: "tequran", controlled_by: "other_seat", exhausted: false },
      torkan: { planet_id: "torkan", controlled_by: "other_seat", exhausted: false },
    },
    units: [
      { unit_type: "carrier", owner: "other_seat", damaged: false },
      { unit_type: "fighter", owner: "other_seat", damaged: false },
      { unit_type: "fighter", owner: "other_seat", damaged: false },
      { unit_type: "infantry", owner: "other_seat", planet: "tequran", damaged: false },
    ],
  },
  "29": {
    system_id: "29",
    command_tokens: [],
    planets: {},
    units: [],
  },
  "30": {
    system_id: "30",
    command_tokens: [],
    planets: {
      centauri: { planet_id: "centauri", controlled_by: null, exhausted: false },
      gral: { planet_id: "gral", controlled_by: null, exhausted: false },
    },
    units: [],
  },
  "31": {
    system_id: "31",
    command_tokens: [],
    planets: {},
    units: [],
  },
  "32": {
    system_id: "32",
    command_tokens: [],
    planets: {},
    units: [],
  },
  "33": {
    system_id: "33",
    command_tokens: [],
    planets: {},
    units: [],
  },

  // Ring 3
  "1": {
    system_id: "1",
    // Pre-activated by Sol: blocked from activation!
    command_tokens: [actor],
    planets: {
      sol_jord: { planet_id: "sol_jord", controlled_by: actor, exhausted: false },
    },
    units: [
      { unit_type: "carrier", owner: actor, damaged: false },
      { unit_type: "destroyer", owner: actor, damaged: false },
      { unit_type: "infantry", owner: actor, planet: "sol_jord", damaged: false },
      { unit_type: "infantry", owner: actor, planet: "sol_jord", damaged: false },
      { unit_type: "space_dock", owner: actor, planet: "sol_jord", damaged: false },
    ],
  },
  "16": {
    system_id: "16",
    // Pre-activated by Hacan
    command_tokens: ["other_seat"],
    planets: {
      arretze: { planet_id: "arretze", controlled_by: "other_seat", exhausted: false },
      hercan: { planet_id: "hercan", controlled_by: "other_seat", exhausted: false },
      kamdorn: { planet_id: "kamdorn", controlled_by: "other_seat", exhausted: false },
    },
    units: [
      { unit_type: "dreadnought", owner: "other_seat", damaged: false },
      { unit_type: "cruiser", owner: "other_seat", damaged: false },
      { unit_type: "space_dock", owner: "other_seat", planet: "arretze", damaged: false },
    ],
  },
  "34": {
    system_id: "34",
    command_tokens: [],
    planets: {},
    units: [],
  },
  "35": {
    system_id: "35",
    command_tokens: [],
    planets: {},
    units: [],
  },
  "65": {
    system_id: "65",
    command_tokens: [],
    planets: {
      primor: { planet_id: "primor", controlled_by: null, exhausted: false },
    },
    units: [],
  },
  "66": {
    system_id: "66",
    command_tokens: [],
    planets: {
      hopes_end: { planet_id: "hopes_end", controlled_by: null, exhausted: false },
    },
    units: [],
  },
};

export const galleryBoard: BoardView = {
  active_system: null,
  map_tiles: mapTiles,
  systems: dynamicSystems,
};
