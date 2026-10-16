// Fictional game data of the click dummy. Nothing here comes from the server.
import type { UnitType } from "../model";

export type Counts = Partial<Record<UnitType, number>>;
export interface MapPlanet {
  id: string;
  name: string;
  res: number;
  inf: number;
  owner: string | null;
  troops?: number;
  tech?: string;
  exhausted?: boolean;
}
export interface MapSystem {
  q: number;
  r: number;
  name: string;
  note?: string;
  home?: string;
  token?: boolean;
  /** A space dock of the player: fighters can stay here without a ship. */
  dock?: boolean;
  anomaly?: "asteroid" | "supernova" | "nebula" | "rift";
  wormhole?: string;
  planets?: MapPlanet[];
  fleets?: Record<string, Counts>;
}
export interface UnitRule {
  name: string;
  plural?: string;
  combat?: number;
  move?: number;
  capacity?: number;
  cost?: number;
  per?: number;
  sustain?: boolean;
  bombard?: number;
  afb?: number;
  cannon?: number;
  ship?: boolean;
  ground?: boolean;
}
export interface Line {
  id: string;
  origin: string;
  type: UnitType;
  n: number;
  from?: string;
  damaged?: boolean;
  move?: number;
  label?: string;
  /** The line exists only in the example that names this scene. */
  scene?: string;
  /** The engine chooses the path of a ship. The dummy takes the path through this system. */
  via?: string;
}
export interface PaySource {
  id: string;
  label: string;
  system?: string;
  res: number;
  inf?: number;
}
export type Side = "att" | "def";

export const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));
export const plural = (count: number, word: string, many = word + "s") =>
  `${count} ${count === 1 ? word : many}`;
export const STEPS = ["Activation", "Movement", "Space combat", "Invasion", "Production"];
export const KEYS = ["activation", "movement", null, "invasion", "production"];
export const SIDES: Side[] = ["att", "def"];
export const SEAT: Record<Side, string> = { att: "sol", def: "hacan" };
export const ROLE: Record<Side, "Attacker" | "Defender"> = { att: "Attacker", def: "Defender" };
export const other = (side: Side): Side => (side === "att" ? "def" : "att");
export const SUPPLY = 3;
// q and r are axial hex coordinates on a five-ring board (61 systems, eight players).
// A system has at most three planets; three systems here have three.
// The active system's planets live in state.planets. `fleets` holds the ships of the other players.
export const MAP: Record<string, MapSystem> = {
  "27": {
    q: 1,
    r: -3,
    name: "Starpoint / New Albion",
    planets: [
      { id: "starpoint", name: "Starpoint", res: 3, inf: 1, owner: "sol" },
      { id: "newalbion", name: "New Albion", res: 1, inf: 1, owner: "hacan", exhausted: true },
    ],
  },
  "1": {
    q: 1,
    r: -4,
    name: "Jord",
    note: "Sol home system",
    home: "sol",
    dock: true,
    planets: [{ id: "jord", name: "Jord", res: 4, inf: 2, owner: "sol" }],
  },
  "23": {
    q: 2,
    r: -4,
    name: "Tar’Mann",
    planets: [{ id: "tarmann", name: "Tar’Mann", res: 1, inf: 1, owner: "sol" }],
  },
  "38": { q: 2, r: -3, name: "Empty space", token: true },
  "40": { q: 1, r: -2, name: "Asteroid field", anomaly: "asteroid" },
  "19": {
    q: 0,
    r: -2,
    name: "Wellon",
    wormhole: "α",
    planets: [{ id: "wellon", name: "Wellon", res: 1, inf: 2, owner: null }],
  },
  "41": { q: 0, r: -3, name: "Gravity rift", anomaly: "rift" },
  "43": { q: 0, r: -4, name: "Supernova", anomaly: "supernova" },
  "42": { q: 3, r: -3, name: "Nebula", anomaly: "nebula" },
  "26": {
    q: 3,
    r: -4,
    name: "Lodor",
    planets: [{ id: "lodor", name: "Lodor", res: 1, inf: 1, owner: "sol" }],
  },
  "36": {
    q: 4,
    r: -4,
    name: "Arnor / Lor / Siig",
    planets: [
      { id: "arnor", name: "Arnor", res: 2, inf: 1, owner: "sol" },
      { id: "lor", name: "Lor", res: 1, inf: 2, owner: "sol" },
      { id: "siig", name: "Siig", res: 0, inf: 2, owner: null },
    ],
  },
  "34": {
    q: 4,
    r: -3,
    name: "Quann",
    wormhole: "α",
    planets: [{ id: "quann", name: "Quann", res: 2, inf: 1, owner: "sol" }],
  },
  "31": {
    q: -1,
    r: -2,
    name: "Vefut",
    planets: [{ id: "vefut", name: "Vefut", res: 2, inf: 2, owner: "sol" }],
  },
  "33": {
    token: true,
    q: 1,
    r: -1,
    name: "Corneeq",
    planets: [{ id: "corneeq", name: "Corneeq", res: 1, inf: 2, owner: "sol" }],
  },
  "22": {
    q: 2,
    r: -2,
    name: "Abyz",
    planets: [
      { id: "abyz", name: "Abyz", res: 3, inf: 0, owner: "hacan", troops: 3, exhausted: true },
    ],
    fleets: { hacan: { carrier: 1, fighter: 2 } },
  },
};
// The rest of the board: [q, r, name, planets as "Name res/inf owner", extra]. Fictional positions.
(
  [
    [
      0,
      0,
      "Mecatol Rex",
      ["Mecatol Rex 1/6 hacan"],
      { fleets: { hacan: { dreadnought: 1, cruiser: 1 } }, note: "Centre of the galaxy" },
    ],
    [
      4,
      -2,
      "Hercant",
      ["Hercant 1/1 hacan", "Arretze 2/0 hacan"],
      { home: "hacan", fleets: { hacan: { carrier: 1, cruiser: 1, fighter: 2 } } },
    ],
    [
      3,
      1,
      "Archon Ren",
      ["Archon Ren 2/3 xxcha", "Archon Tau 1/1 xxcha"],
      { home: "xxcha", fleets: { xxcha: { cruiser: 2 } } },
    ],
    [
      0,
      4,
      "Arc Prime",
      ["Arc Prime 4/0 letnev", "Wren Terra 2/1 letnev"],
      { home: "letnev", fleets: { letnev: { dreadnought: 1, destroyer: 1 } } },
    ],
    [
      -2,
      4,
      "Jol / Nar",
      ["Jol 1/2 jolnar", "Nar 2/3 jolnar"],
      { home: "jolnar", fleets: { jolnar: { carrier: 1, fighter: 2 } } },
    ],
    [
      -4,
      3,
      "Moll Primus",
      ["Moll Primus 4/1 mentak"],
      { home: "mentak", fleets: { mentak: { cruiser: 2, carrier: 1 } } },
    ],
    [
      -4,
      0,
      "Retillion",
      ["Retillion 2/3 yssaril", "Shalloq 1/2 yssaril"],
      { home: "yssaril", fleets: { yssaril: { carrier: 1, fighter: 3 } } },
    ],
    [
      -2,
      -2,
      "Maaluuk",
      ["Maaluuk 0/2 naalu", "Druaa 3/1 naalu"],
      { home: "naalu", fleets: { naalu: { carrier: 1, fighter: 4 } } },
    ],
    // The large fleet of the movement cases is here: its ships leave through the gravity rift.
    [-1, -3, "Everra", ["Everra 1/1 sol"]],
    [3, -2, "Arinam / Meer", ["Arinam 1/2 hacan", "Meer 0/4 hacan R"]],
    [-3, -1, "Empty space"],
    [-2, -1, "Mehar Xull", ["Mehar Xull 1/3 naalu R"]],
    [
      -1,
      -1,
      "Bereg / Lirta IV",
      ["Bereg 3/1 naalu", "Lirta IV 2/3 -"],
      { fleets: { naalu: { cruiser: 1 } } },
    ],
    [0, -1, "Thibah", ["Thibah 1/1 hacan B"], { fleets: { hacan: { destroyer: 1 } } }],
    [2, -1, "Qucen’n / Rarron", ["Qucen’n 1/2 hacan", "Rarron 0/3 -"]],
    [3, -1, "Asteroid field", [], { anomaly: "asteroid" }],
    [4, -1, "Empty space", [], { wormhole: "β" }],
    [-3, 0, "Saudor", ["Saudor 2/2 yssaril"]],
    [-2, 0, "Mellon / Zohbat", ["Mellon 0/2 yssaril", "Zohbat 3/1 naalu"]],
    [-1, 0, "Empty space", [], { fleets: { yssaril: { destroyer: 2 } } }],
    [1, 0, "Dal Bootha / Xxehan", ["Dal Bootha 0/2 xxcha", "Xxehan 1/1 xxcha"]],
    [2, 0, "Nebula", [], { anomaly: "nebula" }],
    [3, 0, "Tequ’ran", ["Tequ’ran 2/0 xxcha"], { fleets: { xxcha: { carrier: 1, fighter: 2 } } }],
    [4, 0, "Empty space"],
    [-4, 1, "Empty space"],
    [-3, 1, "Centauri / Gral", ["Centauri 1/3 yssaril", "Gral 1/1 yssaril B"]],
    [-2, 1, "Gravity rift", [], { anomaly: "rift" }],
    [-1, 1, "Lazar / Sakulag", ["Lazar 1/0 mentak Y", "Sakulag 2/1 -"]],
    [0, 1, "Resculon", ["Resculon 2/0 letnev"]],
    [1, 1, "Lisis II / Ragh", ["Lisis II 1/0 letnev", "Ragh 2/1 xxcha"]],
    [2, 1, "Empty space", [], { fleets: { xxcha: { destroyer: 1 } } }],
    [-4, 2, "Empty space"],
    [-3, 2, "Empty space", [], { wormhole: "β" }],
    [-2, 2, "Vega Major / Vega Minor", ["Vega Major 2/1 mentak", "Vega Minor 1/2 mentak B"]],
    [-1, 2, "Hope’s End", ["Hope’s End 3/0 jolnar"], { fleets: { jolnar: { cruiser: 1 } } }],
    [0, 2, "Supernova", [], { anomaly: "supernova" }],
    [1, 2, "Primor", ["Primor 2/1 letnev"], { fleets: { letnev: { cruiser: 1, destroyer: 1 } } }],
    [2, 2, "Cealdri / Xanhact", ["Cealdri 0/2 xxcha Y", "Xanhact 0/1 -"]],
    [-3, 3, "Atlas", ["Atlas 3/1 mentak"]],
    [
      -2,
      3,
      "Abaddon / Ashtroth / Loki",
      ["Abaddon 1/0 jolnar", "Ashtroth 2/0 mentak", "Loki 1/2 -"],
    ],
    [
      -1,
      3,
      "Rigel I / Rigel II / Rigel III",
      ["Rigel I 0/1 jolnar", "Rigel II 1/2 jolnar G", "Rigel III 1/1 jolnar"],
    ],
    [0, 3, "Empty space", [], { fleets: { letnev: { carrier: 1, fighter: 2 } } }],
    [1, 3, "Perimeter", ["Perimeter 2/1 letnev"]],
    [-4, 4, "Empty space"],
    [-3, 4, "Semlore", ["Semlore 3/2 mentak Y"]],
    [-1, 4, "Vorhal", ["Vorhal 0/2 letnev G"]],
  ] as [number, number, string, string[]?, Partial<MapSystem>?][]
).forEach(([q, r, name, planets = [], extra = {}], index) => {
  MAP[String(50 + index)] = {
    q,
    r,
    name,
    ...extra,
    planets: planets.map((text) => {
      const [, planet, res, inf, owner, tech] = text.match(/^(.+) (\d)\/(\d) (\S+)(?: (\w))?$/)!;
      return {
        id: planet.toLowerCase().replace(/[^a-z]/g, ""),
        name: planet,
        res: Number(res),
        inf: Number(inf),
        owner: owner === "-" ? null : owner,
        tech,
      };
    }),
  };
});
// The player table: what every player needs to see at almost every moment. Key order is seat order.
export const SEATS: Record<string, any> = {
  sol: {
    name: "Jamie",
    faction: "Sol",
    symbol: "●",
    color: "#8cd5ef",
    vp: 6,
    res: [5, 9],
    inf: [3, 7],
    tg: 2,
    comm: [1, 4],
    tokens: [3, 4, 2],
    ac: 4,
    so: [1, 2],
    planets: 8,
    abilities: [
      "Orbital Drop: spend 1 strategy token to place 2 infantry on a planet you control.",
      "Versatile: gain 1 extra command token in the status phase.",
    ],
    tech: ["Neural Motivator", "Antimass Deflectors", "Gravity Drive", "Cruiser II"],
    leaders: "Agent ready · Commander unlocked · Hero locked",
  },
  hacan: {
    name: "Alex",
    faction: "Hacan",
    symbol: "◆",
    color: "#d69a75",
    vp: 7,
    speaker: true,
    res: [2, 8],
    inf: [6, 13],
    tg: 7,
    comm: [0, 6],
    tokens: [2, 5, 1],
    ac: 2,
    so: [0, 1],
    planets: 10,
    abilities: [
      "Masters of Trade: no token cost for the Trade secondary.",
      "Guild Ships: may trade with players who are not neighbours.",
      "Arbiters: may trade action cards.",
    ],
    tech: ["Sarween Tools", "Antimass Deflectors", "Production Biomes"],
    leaders: "Agent exhausted · Commander locked · Hero locked",
  },
  xxcha: {
    name: "Blair",
    faction: "Xxcha",
    symbol: "▲",
    color: "#8dd2b0",
    vp: 5,
    res: [4, 6],
    inf: [5, 9],
    tg: 4,
    comm: [2, 4],
    tokens: [1, 3, 2],
    ac: 5,
    so: [1, 1],
    planets: 7,
    abilities: [
      "Peace Accords: after Diplomacy resolves, gain control of an adjacent planet without units.",
      "Quash: spend 1 strategy token to discard an agenda.",
    ],
    tech: ["Graviton Laser System", "Plasma Scoring", "Magen Defense Grid"],
    leaders: "Agent ready · Commander unlocked · Hero locked",
  },
  letnev: {
    name: "Casey",
    faction: "Letnev",
    symbol: "■",
    color: "#ec7f8f",
    vp: 4,
    res: [9, 13],
    inf: [1, 4],
    tg: 1,
    comm: [0, 2],
    tokens: [2, 6, 1],
    ac: 0,
    so: [0, 2],
    planets: 7,
    abilities: [
      "Munitions Reserves: spend 2 trade goods at the start of a combat round to re-roll your dice.",
      "Armada: your fleet limit is 2 higher than your fleet pool.",
    ],
    tech: ["Antimass Deflectors", "Plasma Scoring", "Dreadnought II"],
    leaders: "Agent ready · Commander locked · Hero locked",
  },
  jolnar: {
    name: "Bartholomew",
    faction: "Jol-Nar",
    symbol: "✚",
    color: "#b79cf0",
    vp: 6,
    res: [3, 7],
    inf: [2, 9],
    tg: 3,
    comm: [4, 4],
    tokens: [2, 3, 3],
    ac: 6,
    so: [1, 3],
    planets: 6,
    abilities: [
      "Fragile: −1 to all combat rolls.",
      "Brilliant: research 2 technologies with the Technology secondary.",
      "Analytical: ignore 1 prerequisite when you research.",
    ],
    tech: [
      "Neural Motivator",
      "Antimass Deflectors",
      "Sarween Tools",
      "Plasma Scoring",
      "Gravity Drive",
      "Fleet Logistics",
    ],
    leaders: "Agent exhausted · Commander unlocked · Hero locked",
  },
  mentak: {
    name: "Erin",
    faction: "Mentak",
    symbol: "✖",
    color: "#e6c15c",
    vp: 3,
    passed: true,
    res: [0, 12],
    inf: [0, 6],
    tg: 5,
    comm: [0, 2],
    tokens: [0, 4, 1],
    ac: 1,
    so: [0, 1],
    planets: 7,
    abilities: [
      "Ambush: at the start of a space combat, roll for up to 2 cruisers or destroyers.",
      "Pillage: take 1 trade good from a neighbour who gains or trades goods.",
    ],
    tech: ["Sarween Tools", "Plasma Scoring", "Cruiser II"],
    leaders: "Agent ready · Commander locked · Hero locked",
  },
  yssaril: {
    name: "Maximilian Alexander",
    faction: "Yssaril",
    symbol: "♣",
    color: "#f0a6d0",
    vp: 5,
    res: [2, 5],
    inf: [4, 9],
    tg: 0,
    comm: [3, 3],
    tokens: [3, 3, 0],
    ac: 9,
    so: [1, 2],
    planets: 5,
    abilities: [
      "Stall Tactics: discard 1 action card as an action.",
      "Scheming: draw 1 extra action card, then discard 1.",
      "Crafty: no limit to action cards in hand.",
    ],
    tech: ["Neural Motivator", "Transit Diodes", "Mageon Implants"],
    leaders: "Agent ready · Commander unlocked · Hero locked",
  },
  naalu: {
    name: "Gale",
    faction: "Naalu",
    symbol: "★",
    color: "#b9d86a",
    vp: 4,
    res: [4, 9],
    inf: [3, 9],
    tg: 2,
    comm: [1, 3],
    tokens: [1, 4, 2],
    ac: 4,
    so: [0, 2],
    planets: 6,
    abilities: [
      "Telepathic: you are always first in initiative order.",
      "Foresight: spend 1 strategy token to move a ship out of a system that an opponent moves into.",
    ],
    tech: ["Neural Motivator", "Sarween Tools", "Hybrid Crystal Fighter I"],
    leaders: "Agent ready · Commander locked · Hero locked",
  },
};
export const SEAT_IDS = Object.keys(SEATS);
export const CARDS = [
  "Leadership",
  "Diplomacy",
  "Politics",
  "Construction",
  "Trade",
  "Warfare",
  "Technology",
  "Imperial",
];
/** The types of the planets of the demo. A planet that is not here has none (a home planet). */
export const PLANET_TYPES: Record<string, ("cultural" | "hazardous" | "industrial")[]> = {
  starpoint: ["hazardous"],
  newalbion: ["industrial"],
  tarmann: ["industrial"],
  wellon: ["industrial"],
  lodor: ["cultural"],
  arnor: ["industrial"],
  lor: ["industrial"],
  siig: ["hazardous"],
  quann: ["cultural"],
  vefut: ["hazardous"],
  corneeq: ["cultural"],
  abyz: ["hazardous"],
};
export const ATTACHMENTS: Record<string, { name: string; effect: string; text: string }[]> = {
  quann: [
    {
      name: "Dyson Sphere",
      effect: "+2 / +1",
      text: "This planet's resource value is increased by 2 and its influence value is increased by 1.",
    },
  ],
  vefut: [
    {
      name: "Warfare Research Facility",
      effect: "+1 / +1",
      text: "This planet has a red technology specialty. If this planet already has a technology specialty, this planet's resource and influence values are each increased by 1 instead.",
    },
  ],
};
export const TECH: Record<string, string> = {
  tarmann: "G",
  vefut: "R",
  arnor: "B",
  wellon: "Y",
  newalbion: "G",
  ...Object.fromEntries(
    Object.values(MAP).flatMap((system) =>
      (system.planets || [])
        .filter((planet) => planet.tech)
        .map((planet) => [planet.id, planet.tech]),
    ),
  ),
};
export const SIM: Record<string, string> = {
  hacan: "Followed · bought 1 token for 3 influence",
  xxcha: "Passed",
  letnev: "Passed",
  jolnar: "Followed · bought 2 tokens for 6 influence",
  mentak: "Passed",
  yssaril: "Followed · bought 1 token for 3 influence",
  naalu: "Passed",
};
export const HEX = [
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
  [1, -1],
  [-1, 1],
];
// A player can activate every system without their command token. The demo scripts the action for these two.
export const SCRIPTED = ["27", "19"];
export const ADJ = Object.fromEntries(
  Object.keys(MAP).map((id) => [
    id,
    Object.keys(MAP).filter((other) =>
      HEX.some(([q, r]) => MAP[other].q === MAP[id].q + q && MAP[other].r === MAP[id].r + r),
    ),
  ]),
);
export const NEAR = Object.fromEntries(
  Object.keys(MAP).map((id) => [
    id,
    [
      ...ADJ[id],
      ...Object.keys(MAP).filter(
        (other) => other !== id && MAP[id].wormhole && MAP[other].wormhole === MAP[id].wormhole,
      ),
    ],
  ]),
);
export const ANOMALY: Record<string, { label: string; rule: string }> = {
  asteroid: { label: "Asteroid field", rule: "Ships cannot move into or through it." },
  supernova: { label: "Supernova", rule: "Ships cannot move into or through it." },
  nebula: {
    label: "Nebula",
    rule: "Ships may move in, not through. A ship that starts here has move 1. The defender gets +1 in space combat.",
  },
  rift: {
    label: "Gravity rift",
    rule: "A ship that leaves it gets +1 move and rolls one die: destroyed on 1–3.",
  },
};
export const P: Record<string, { name: string; faction: string; fleet: number; hand: string[] }> = {
  sol: {
    name: "Jamie",
    faction: "Sol",
    fleet: SUPPLY,
    hand: ["Direct Hit", "Morale Boost", "Sabotage", "Ghost Ship"],
  },
  hacan: { name: "Alex", faction: "Hacan", fleet: 4, hand: ["Shields Holding", "Skilled Retreat"] },
};
// Key order is the row order of every unit list.
// Key order is the display order of a force.
export const U: Record<UnitType, UnitRule> = {
  flagship: {
    name: "Flagship",
    combat: 5,
    move: 1,
    capacity: 3,
    cost: 8,
    sustain: true,
    ship: true,
  },
  warsun: {
    name: "War sun",
    combat: 3,
    move: 2,
    capacity: 6,
    cost: 12,
    sustain: true,
    bombard: 3,
    ship: true,
  },
  dreadnought: {
    name: "Dreadnought",
    combat: 5,
    move: 1,
    capacity: 1,
    cost: 4,
    sustain: true,
    bombard: 5,
    ship: true,
  },
  carrier: { name: "Carrier", combat: 9, move: 1, capacity: 4, cost: 3, ship: true },
  cruiser: { name: "Cruiser", combat: 7, move: 2, cost: 2, ship: true },
  destroyer: { name: "Destroyer", combat: 9, move: 2, cost: 1, afb: 9, ship: true },
  fighter: { name: "Fighter", combat: 9, cost: 1, per: 2 },
  mech: { name: "Mech", combat: 6, cost: 2, sustain: true, ground: true },
  infantry: { name: "Infantry", plural: "Infantry", combat: 8, cost: 1, per: 2, ground: true },
  pds: { name: "PDS", plural: "PDS", cannon: 6 },
  dock: { name: "Space dock" },
};
export const LOSS_ORDER: UnitType[] = [
  "fighter",
  "infantry",
  "destroyer",
  "carrier",
  "cruiser",
  "mech",
  "dreadnought",
  "flagship",
  "warsun",
];
export const STOCK: Counts = {
  fighter: 6,
  infantry: 7,
  destroyer: 7,
  cruiser: 7,
  carrier: 3,
  dreadnought: 4,
  mech: 3,
};
export const PAY: PaySource[] = [
  { id: "goods", label: "Spend 2 trade goods", res: 2 },
  ...Object.entries(MAP).flatMap(([system, { planets = [] }]) =>
    planets
      .filter((planet) => planet.owner === "sol")
      .map((planet) => ({
        id: planet.id,
        label: `Exhaust ${planet.name}`,
        system,
        res: planet.res,
        inf: planet.inf,
      })),
  ),
];
// Ships, and the fighters and ground forces a ship can load where it starts or on its way.
export const LINES: Line[] = [
  { id: "j-dreadnought", origin: "1", type: "dreadnought", n: 1 },
  { id: "j-carrier", origin: "1", type: "carrier", n: 1 },
  { id: "j-cruiser", origin: "1", type: "cruiser", n: 1 },
  { id: "j-fighter", origin: "1", type: "fighter", n: 4, from: "space" },
  { id: "j-mech", origin: "1", type: "mech", n: 1, from: "Jord" },
  { id: "j-infantry", origin: "1", type: "infantry", n: 5, from: "Jord" },
  { id: "t-dreadnought", origin: "23", type: "dreadnought", n: 1, damaged: true },
  { id: "t-dreadnought-ok", origin: "23", type: "dreadnought", n: 1, scene: "cases" },
  { id: "t-destroyer", origin: "23", type: "destroyer", n: 1 },
  { id: "t-infantry", origin: "23", type: "infantry", n: 2, from: "Tar’Mann" },
  { id: "v-dreadnought", origin: "31", type: "dreadnought", n: 1 },
  { id: "v-carrier", origin: "31", type: "carrier", n: 1, scene: "cases" },
  { id: "v-destroyer", origin: "31", type: "destroyer", n: 1, via: "41" },
  { id: "v-infantry", origin: "31", type: "infantry", n: 1, from: "Vefut" },
  { id: "l-carrier", origin: "26", type: "carrier", n: 1, via: "23" },
  { id: "l-dreadnought", origin: "26", type: "dreadnought", n: 1, via: "38", scene: "cases" },
  { id: "l-fighter", origin: "26", type: "fighter", n: 2, from: "space" },
  { id: "l-infantry", origin: "26", type: "infantry", n: 2, from: "Lodor" },
  { id: "q-cruiser", origin: "34", type: "cruiser", n: 1, move: 3, label: "Cruiser II" },
  { id: "s-destroyer", origin: "38", type: "destroyer", n: 1 },
  { id: "s-fighter", origin: "38", type: "fighter", n: 2, from: "space" },
  { id: "n-destroyer", origin: "42", type: "destroyer", n: 1 },
  { id: "c-cruiser", origin: "33", type: "cruiser", n: 1 },
  { id: "a-infantry", origin: "36", type: "infantry", n: 1, from: "Arnor" },
  { id: "w-fighter", origin: "19", type: "fighter", n: 2, from: "space", scene: "cases" },
  // Many ships of one kind, and more cargo than their holds take.
  { id: "e-carrier", origin: "58", type: "carrier", n: 4, scene: "cases" },
  { id: "e-dreadnought", origin: "58", type: "dreadnought", n: 4, scene: "cases" },
  { id: "e-fighter", origin: "58", type: "fighter", n: 8, from: "space", scene: "cases" },
  { id: "e-mech", origin: "58", type: "mech", n: 2, from: "Everra", scene: "cases" },
  { id: "e-infantry", origin: "58", type: "infantry", n: 8, from: "Everra", scene: "cases" },
];
export const lineById = Object.fromEntries(LINES.map((line) => [line.id, line]));
/** The lines of one example: the lines of every example, and those of its scene. */
export const linesOf = (state: { scene?: string }) =>
  LINES.filter((line) => !line.scene || line.scene === state.scene);
export const RETREATS: Record<Side, string[]> = {
  att: ["1", "23"],
  def: ["19", "41"],
};
export const SPECS: Record<
  string,
  (type: UnitType) => false | undefined | 0 | { target: number; dice?: number }
> = {
  combat: (type: UnitType) => (U[type].ship || type === "fighter") && { target: U[type].combat! },
  ground: (type: UnitType) => U[type].ground && { target: U[type].combat! },
  afb: (type: UnitType) => U[type].afb && { target: U[type].afb!, dice: 2 },
  bombard: (type: UnitType) => U[type].bombard && { target: U[type].bombard! },
  cannon: (type: UnitType) => U[type].cannon && { target: U[type].cannon! },
};
export const MORALE = { value: 1, source: "Morale Boost" };
export const baseData = {
  activation: { system: "27" },
  movement: { "j-carrier": 1, "j-dreadnought": 1, "j-cruiser": 1, "j-fighter": 2, "j-infantry": 3 },
  invasion: { "starpoint:infantry": 1, "newalbion:infantry": 2 },
  production: {
    units: { fighter: 2, infantry: 2 },
    pay: { goods: 2 },
    place: { infantry: "starpoint", mech: "starpoint" },
  },
};
export const examples: Record<string, any> = {
  "draft-combat": {
    mode: "draft",
    route: "hostile",
    tip: "The draft stops before the first combat roll. Open Movement to change the fleet; the odds follow.",
  },
  "draft-start": {
    mode: "draft",
    route: "hostile",
    until: 0,
    tip: "Click any system on the board, or use Find system. The demo scripts Starpoint and Wellon. Wellon is reached through the gravity rift or the α wormhole.",
  },
  "draft-movement": {
    mode: "draft",
    route: "hostile",
    until: 1,
    movement: {
      "j-dreadnought": 0,
      "j-cruiser": 0,
      "j-infantry": 2,
      "l-carrier": 1,
      "l-fighter": 2,
      "l-carrier>t-infantry": 2,
      "q-cruiser": 1,
    },
    tip: "Three origins. The Lodor carrier picks up infantry at Tar’Mann on its way. Choose a system on the map to move its ships.",
  },
  "draft-movement-cases": {
    mode: "draft",
    route: "hostile",
    until: 1,
    scene: "cases",
    movement: {
      "t-dreadnought-ok": 1,
      "l-carrier": 1,
      "l-infantry": 2,
      "l-carrier>t-infantry": 1,
      "v-carrier": 1,
      "v-infantry": 1,
      "v-destroyer": 1,
      "q-cruiser": 1,
    },
    tip: "Every edge case of movement on one board: origins one, two and three systems away, holds that are full and holds with room, a pickup, cargo that no ship can take, Gravity Drive for one of two ships, rift rolls, a damaged ship, command tokens, fleet supply, and at Everra four carriers and four dreadnoughts with more cargo than they hold.",
  },
  "draft-rift": {
    mode: "draft",
    route: "hostile",
    movement: {
      "j-dreadnought": 0,
      "j-cruiser": 0,
      "j-infantry": 2,
      "v-dreadnought": 1,
      "v-destroyer": 1,
      "v-infantry": 1,
    },
    tip: "Two ships leave the gravity rift, so the draft stops at the rift roll. The game chooses the path of a ship: the destroyer leaves the rift too.",
  },
  "draft-invasion": {
    mode: "draft",
    route: "ground",
    until: 3,
    movement: { "j-dreadnought": 0, "j-fighter": 1, "j-infantry": 2, "j-mech": 1 },
    invasion: { "starpoint:infantry": 1, "newalbion:infantry": 1, "newalbion:mech": 1 },
    tip: "Distribute forces and watch the projected odds. The draft stops at ground combat.",
  },
  "draft-production": {
    mode: "draft",
    route: "friendly",
    until: 4,
    movement: { "j-cruiser": 0 },
    tip: "Build a cart, pay, and place. Pay on the map: click the bright planets.",
  },
  "draft-review": {
    mode: "draft",
    route: "friendly",
    stale: true,
    tip: "A recorded cruiser has left Jord. The affected row is marked with its fix.",
  },
  "draft-cannon": {
    mode: "draft",
    route: "cannon",
    tip: "Space cannon offense belongs to Movement, so the draft stops there. Apply to see the roll.",
  },
  "live-combat": {
    mode: "live",
    route: "hostile",
    until: 2,
    tip: "Roll, play Direct Hit or Morale Boost, then assign hits on the rows. Try “View as” Alex.",
  },
  // A layout test: both sides have every ship type. Fleet supply is not applied.
  "live-combat-full": {
    mode: "live",
    route: "hostile",
    until: 2,
    forces: {
      att: {
        flagship: 1,
        warsun: 1,
        dreadnought: 2,
        carrier: 1,
        cruiser: 2,
        destroyer: 2,
        fighter: 4,
      },
      def: {
        flagship: 1,
        warsun: 1,
        dreadnought: 1,
        carrier: 2,
        cruiser: 1,
        destroyer: 2,
        fighter: 3,
      },
    },
    tip: "Every ship type on both sides, to check that the battle table fits. Fleet supply is not applied here.",
  },
  "live-invasion": {
    mode: "live",
    route: "hostile",
    until: 4,
    hold: true,
    show: 3,
    tip: "Ground combat waits for Alex. Earlier results stay one click away in the stepper.",
  },
  "h-mining": {
    mode: "live",
    kind: "component",
    flow: {
      card: "Mining Initiative",
      owner: "xxcha",
      stage: "done",
      result: "Blair gained 3 trade goods for Archon Ren. Trade goods 1 → 4.",
    },
  },
  "h-leadership": {
    mode: "live",
    kind: "strategic",
    flow: {
      card: "Leadership",
      owner: "hacan",
      stage: "done",
      primaryResult: "Alex gained 3 command tokens and bought 1 for 3 influence.",
      order: [...SEAT_IDS.slice(2), "sol"],
      turn: 7,
      results: { ...SIM, xxcha: "Followed · bought 2 tokens for 6 influence", sol: "Passed" },
      mine: { buy: 0, pools: { t: 0, f: 0, s: 0 }, pay: {}, resolved: true },
    },
  },
  "h-cannon": { mode: "live", route: "cannon", show: 1 },
  summary: {
    mode: "live",
    route: "hostile",
    show: 2,
    tip: "Every step keeps its result. Step through barrage and rounds; dice are grouped per unit type.",
  },
};
/** Action cards that are played in a reaction window, with their text. */
export const REACTION_CARDS: Record<string, string> = {
  "Direct Hit":
    "After another player’s ship uses Sustain Damage to cancel a hit of your units: destroy that ship.",
  "Morale Boost":
    "At the start of a combat round: apply +1 to the result of each of your unit’s combat rolls during this round.",
  Sabotage:
    "When another player plays an action card other than Sabotage: cancel that action card.",
};

/** Action cards that are played as a component action, with their text. */
export const ACTION_CARDS: Record<string, string> = {
  "Mining Initiative": "Gain trade goods equal to the resource value of 1 planet you control.",
  "Unexpected Action":
    "Remove 1 of your command tokens from the game board and return it to your reinforcements.",
  Spy: "Choose 1 player. That player gives you 1 random action card from their hand.",
  "Ghost Ship":
    "Place 1 destroyer from your reinforcements in a non-home system that contains a wormhole and no ships of other players.",
  "Focused Research": "Spend 4 trade goods to research 1 technology.",
  Plague:
    "Choose 1 planet that another player controls. Roll 1 die for each infantry on it. Destroy 1 infantry for each result of 6 or greater.",
};
/** When a card of a reaction window is played, roughly. The other cards are in "Action". */
export const CARD_GROUPS: Record<string, string> = {
  "Direct Hit": "Combat",
  "Morale Boost": "Combat",
  Sabotage: "Any time",
};
/** The action cards of the demo that are in the discard pile. */
export const DISCARDED_CARDS = ["Morale Boost", "Sabotage", "Plague"];
/**
 * Where the points of the demo come from, beside the objectives of `OBJECTIVES` and the scored
 * secrets of a seat (`so`): more public objectives with who scored them, and the other sources
 * with the points of each seat. `from` is whose Support for the Throne a seat has. With them the
 * points of every seat sum to its `vp`.
 */
export const SCORE: {
  public: { name: string; text: string; worth: number; scored: string[] }[];
  other: { name: string; points: Record<string, number>; from?: Record<string, string[]> }[];
} = {
  public: [
    {
      name: "Diversify Research",
      text: "Own 2 technologies in each of 2 colors.",
      worth: 1,
      scored: ["xxcha", "letnev", "jolnar", "mentak", "yssaril", "naalu"],
    },
    {
      name: "Sway the Council",
      text: "Spend 8 influence.",
      worth: 1,
      scored: ["hacan", "mentak", "yssaril", "naalu"],
    },
    {
      name: "Master the Sciences",
      text: "Own 2 technologies in each of 4 colors.",
      worth: 2,
      scored: ["sol", "jolnar"],
    },
    {
      name: "Galvanize the People",
      text: "Spend a total of 6 tokens from your tactic and/or strategy pools.",
      worth: 2,
      scored: ["letnev", "jolnar", "yssaril", "naalu"],
    },
  ],
  other: [
    { name: "Custodians of Mecatol Rex", points: { hacan: 1 } },
    { name: "Imperial · Mecatol Rex", points: { sol: 1, hacan: 2 } },
    {
      name: "Support for the Throne",
      points: { sol: 1, mentak: 1, naalu: 1 },
      from: { sol: ["xxcha"], mentak: ["naalu"], naalu: ["letnev"] },
    },
    { name: "Agenda · Mutiny", points: { hacan: -1, naalu: -1 } },
    { name: "Agenda · Political Censure", points: { xxcha: 1 } },
    { name: "Relic · Shard of the Throne", points: { letnev: 1 } },
    { name: "Action card · Imperial Rider", points: { naalu: 1 } },
  ],
};
/** What the agendas affect, in the order of the sheet. */
export const AGENDA_GROUPS = [
  "Victory points and objectives",
  "Ministers and offices",
  "Units and combat",
  "Map and movement",
  "Laws and votes",
];
/**
 * The agendas of the demo, a short deck. `passed` is the outcome of a law in play: a seat, "For"
 * or "Against". The demo follows its points: Political Censure is with Xxcha.
 */
export const AGENDAS: {
  name: string;
  group: string;
  kind: string;
  text: string;
  against?: string;
  passed?: string;
}[] = [
  {
    name: "Political Censure",
    group: "Victory points and objectives",
    kind: "Law · Elect Player",
    text: "The elected player gains this card and 1 victory point. The elected player cannot play action cards. If the owner of this card loses this card, they lose 1 victory point.",
    passed: "xxcha",
  },
  {
    name: "Fleet Regulations",
    group: "Units and combat",
    kind: "Law · For/Against",
    text: "For: Each player cannot have more than 4 tokens in their fleet pool.",
    against:
      "Against: Each player places 1 command token from their reinforcements in their fleet pool.",
    passed: "For",
  },
  {
    name: "Seed of an Empire",
    group: "Victory points and objectives",
    kind: "Directive · For/Against",
    text: "For: The player with the most victory points gains 1 victory point.",
    against: "Against: The player with the fewest victory points gains 1 victory point.",
  },
  {
    name: "Minister of War",
    group: "Ministers and offices",
    kind: "Law · Elect Player",
    text: "The elected player gains this card. The owner of this card may discard this card after performing an action to remove 1 of their command tokens from the game board and return it to their reinforcements; then they may perform 1 additional action.",
  },
  {
    name: "Imperial Arbiter",
    group: "Ministers and offices",
    kind: "Law · Elect Player",
    text: "The elected player gains this card. At the end of the strategy phase, the owner of this card may discard this card to swap 1 of their strategy cards with 1 of another player's strategy cards.",
  },
  {
    name: "Articles of War",
    group: "Units and combat",
    kind: "Law · For/Against",
    text: "For: All mechs lose their printed abilities except for SUSTAIN DAMAGE.",
    against: 'Against: Each player that voted "For" gains 3 trade goods.',
  },
  {
    name: "Enforced Travel Ban",
    group: "Map and movement",
    kind: "Law · For/Against",
    text: "For: Alpha and beta wormholes have no effect during movement.",
    against: "Against: Destroy each PDS in or adjacent to a system that contains a wormhole.",
  },
  {
    name: "Judicial Abolishment",
    group: "Laws and votes",
    kind: "Directive · Elect Law",
    text: "(When this agenda is revealed, if there are no laws in play, discard this card and reveal another agenda from the top of the deck.) Discard the elected law from play.",
  },
];
/** What the secret objectives ask for, in the order of the sheet. */
export const SECRET_GROUPS = [
  "Planets you control",
  "Ships on the board",
  "Units and structures",
  "Technology and cards",
  "Win a combat",
  "Destroy units",
  "Agenda and table",
];
/** The secret objectives of the demo: the group, the phase, and who holds or scored it. */
export const SECRETS: {
  name: string;
  group: string;
  phase: string;
  text: string;
  held?: string;
  scored?: string;
}[] = [
  {
    name: "Forge an Alliance",
    group: "Planets you control",
    phase: "Status",
    text: "Control 4 cultural planets.",
    scored: "xxcha",
  },
  {
    name: "Occupy the Seat of the Empire",
    group: "Planets you control",
    phase: "Status",
    text: "Control Mecatol Rex and have 3 or more ships in its system.",
    held: "sol",
  },
  {
    name: "Seize an Icon",
    group: "Planets you control",
    phase: "Status",
    text: "Control a legendary planet.",
  },
  {
    name: "Become the Gatekeeper",
    group: "Ships on the board",
    phase: "Status",
    text: "Have 1 or more ships in a system that contains an alpha wormhole and 1 or more ships in a system that contains a beta wormhole.",
  },
  {
    name: "Threaten Enemies",
    group: "Ships on the board",
    phase: "Status",
    text: "Have 1 or more ships in a system that is adjacent to another player’s home system.",
    scored: "hacan",
  },
  {
    name: "Fuel the War Machine",
    group: "Units and structures",
    phase: "Status",
    text: "Have 3 space docks on the game board.",
    held: "sol",
  },
  {
    name: "Gather a Mighty Fleet",
    group: "Units and structures",
    phase: "Status",
    text: "Have 5 dreadnoughts on the game board.",
  },
  {
    name: "Master the Laws of Physics",
    group: "Technology and cards",
    phase: "Status",
    text: "Own 4 technologies of the same color.",
    scored: "jolnar",
  },
  {
    name: "Brave the Void",
    group: "Win a combat",
    phase: "Action",
    text: "Win a combat in an anomaly.",
  },
  {
    name: "Unveil Flagship",
    group: "Win a combat",
    phase: "Action",
    text: "Win a space combat in a system that contains your flagship. You cannot score this objective if your flagship is destroyed in the combat.",
  },
  {
    name: "Destroy Their Greatest Ship",
    group: "Destroy units",
    phase: "Action",
    text: "Destroy another player’s war sun or flagship.",
    scored: "sol",
  },
  {
    name: "Dictate Policy",
    group: "Agenda and table",
    phase: "Agenda",
    text: "There are 3 or more laws in play.",
  },
  {
    name: "Prove Endurance",
    group: "Agenda and table",
    phase: "Action",
    text: "Be the last player to pass during a game round.",
    scored: "mentak",
  },
];
export const FACES = [
  7, 2, 9, 4, 10, 3, 8, 1, 6, 9, 5, 2, 8, 4, 10, 1, 7, 3, 6, 9, 2, 5, 8, 4, 1, 10, 6, 3, 7, 9,
];
export const HISTORY: {
  id: string;
  round: number;
  seat: string;
  type: string;
  example: string;
  label: string;
  state?: any;
}[] = [
  {
    id: "h1",
    round: 3,
    seat: "xxcha",
    type: "Component",
    example: "h-mining",
    label: "Blair · Mining Initiative",
  },
  {
    id: "h2",
    round: 3,
    seat: "hacan",
    type: "Strategic",
    example: "h-leadership",
    label: "Alex · Leadership",
  },
  {
    id: "h3",
    round: 2,
    seat: "sol",
    type: "Tactical",
    example: "summary",
    label: "Jamie · Tactical #27",
  },
  {
    id: "h4",
    round: 2,
    seat: "sol",
    type: "Tactical",
    example: "h-cannon",
    label: "Jamie · Tactical #27",
  },
];
