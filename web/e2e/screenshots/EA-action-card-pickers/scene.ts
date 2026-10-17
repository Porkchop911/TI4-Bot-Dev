import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Page, TestInfo } from "@playwright/test";
import type { BoardView, ChoiceOptionDto, PlayerView } from "../../../src/protocol/types";
import { openMockedGame, type MockChoice, type MockGameOptions } from "../_shared/mockGame";
import { galleryBoard } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

/**
 * Which side of the change a run is. `before` is taken on the commit without the pickers
 * (the audit branch) and uses the decisions the engine sent THEN; `after` uses what it sends now.
 */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
export const legacy = prefix === "before";

export const viewports = [
  { id: "desktop", width: 1440, height: 900 },
  { id: "phone", width: 390, height: 844 },
] as const;
export type Viewport = (typeof viewports)[number];

// Participant ids as the engine writes them (player_ and 64 hex characters), so the labels carry
// the raw ids the way real decisions do.
export const RIVAL = `player_${"b793e54282b7a62cf6b1d7876eb".padEnd(64, "0")}`;
export const THIRD = `player_${"7b2a83550545c580c4b919fa03f".padEnd(64, "a")}`;
export const ME = `player_${"1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809".slice(0, 64)}`;
export const NAMES = { [RIVAL]: "Mira", [THIRD]: "Kade" };

const retarget = <T,>(value: T): T =>
  JSON.parse(JSON.stringify(value).replaceAll("other_seat", RIVAL).replaceAll("gallery_rival", RIVAL).replaceAll("gallery_seat", ME));

/** The gallery board with the rival's id in place of its placeholder, plus overrides for single systems. */
export function sceneBoard(patch: Record<string, Partial<BoardView["systems"][string]>> = {}): BoardView {
  const board = retarget(galleryBoard) as BoardView;
  for (const [id, over] of Object.entries(patch)) {
    board.systems[id] = { ...board.systems[id], ...over };
  }
  return board;
}

export const players = (over: Partial<PlayerView> = {}, second: Partial<PlayerView> = {}, third: Partial<PlayerView> = {}): PlayerView[] => [
  playerWithHand({ id: ME, ...over }),
  { ...opponent, id: RIVAL, ...second },
  { ...opponent, id: THIRD, faction: "letnev", victory_points: 4, ...third },
];

export interface Scene {
  choice?: MockChoice | null;
  game?: MockGameOptions;
  /** Runs after the page opened, before the shot (e.g. click a planet chip). */
  after?: (page: Page) => Promise<void>;
  /** Test id that must be on screen before shooting. */
  ready?: string;
  /** A whole page rather than the dialog: skip waiting for the decision dialog. */
  hold?: number;
}

export async function capture(page: Page, testInfo: TestInfo, name: string, viewport: Viewport, scene: Scene) {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await openMockedGame(page, {
    seat: ME,
    players: players(),
    nicknames: NAMES,
    board: sceneBoard(),
    ...scene.game,
    ...(scene.choice === undefined ? {} : { choice: scene.choice }),
  });
  if (scene.ready) await page.getByTestId(scene.ready).first().waitFor();
  await page.waitForTimeout(scene.hold ?? 800);
  await scene.after?.(page);
  await page.waitForTimeout(300);
  await shot(page, testInfo, `${prefix}-${name}-${viewport.id}`);
}

const option = (id: string, label: string, kind: string, payload?: Record<string, unknown>): ChoiceOptionDto =>
  ({ id, label, kind, ...(payload ? { payload } : {}) }) as ChoiceOptionDto;

const agendas = JSON.parse(
  readFileSync(join(process.cwd(), "../crates/ti4-content/content/agendas.json"), "utf8"),
) as Array<Record<string, string>>;
/** An agenda as the engine prints it into details (`agenda_details`). */
const printed = (alias: string, outcome?: string) => {
  const card = agendas.find((entry) => entry.alias === alias)!;
  return { id: alias, name: card.name, type: card.type, target: card.target, text1: card.text1, text2: card.text2, ...(outcome ? { outcome } : {}) };
};
export { printed };

// ---- the scenes ------------------------------------------------------------------------------

export const plagiarize: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "Plagiarize: which technology to steal",
    context: { subtype: "plagiarize_pick_technology", source: { ActionCard: "plagiarize" } },
    options: [
      option(`${RIVAL}|det`, `${RIVAL}'s Dark Energy Tap`, "technology"),
      option(`${RIVAL}|gls`, `${RIVAL}'s Graviton Laser System`, "technology"),
      option(`${THIRD}|amd`, `${THIRD}'s Automated Defense Turrets`, "technology"),
    ],
  },
};

export const lucky: Scene = {
  ready: "pending-choice-dialog",
  game: {
    board: sceneBoard({
      "28": {
        units: [
          ...sceneBoard().systems["28"].units,
          { unit_type: "cruiser", owner: RIVAL, damaged: false },
          { unit_type: "dreadnought2", owner: RIVAL, damaged: true },
        ],
      },
      "27": { units: [{ unit_type: "destroyer", owner: THIRD, damaged: false }] },
    }),
  },
  choice: {
    prompt: "Lucky Shot: destroy which ship",
    context: { subtype: "lucky_pick_ship", source: { ActionCard: "lucky" } },
    options: [
      option("28|4", `destroy ${RIVAL}'s cruiser in 28`, "ship", legacy ? undefined : { system: "28" }),
      option("28|5", `destroy ${RIVAL}'s dreadnought2 in 28`, "ship", legacy ? undefined : { system: "28" }),
      option("27|0", `destroy ${THIRD}'s destroyer in 27`, "ship", legacy ? undefined : { system: "27" }),
    ],
    ...(legacy
      ? {}
      : {
          details: {
            units: {
              "28|4": { system: "28", planet: null, unit: "cruiser", owner: RIVAL, damaged: false, cost: 2 },
              "28|5": { system: "28", planet: null, unit: "dreadnought2", owner: RIVAL, damaged: true, cost: 4 },
              "27|0": { system: "27", planet: null, unit: "destroyer", owner: THIRD, damaged: false, cost: 1 },
            },
          },
        }),
  },
};

const mat = ["pok3politics", "pok4construction", "pok6warfare", "pok7technology"];
const cardName = (id: string) => id.replace(/^(pok|te)\d/, "").replace(/^./, (c) => c.toUpperCase());
const cardLabel = (id: string) => `${id.match(/\d/)![0]}. ${cardName(id)}`;
export const disgrace: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "choose a different strategy card",
    context: { subtype: "public_disgrace_choose_card", source: { ActionCard: "public_disgrace" } },
    options: mat.map((id) => option(id, cardLabel(id), "strategy_card")),
    ...(legacy ? {} : { details: { first: "pok5trade", played_by: RIVAL } }),
  },
};

const strategyOptions = (): ChoiceOptionDto[] => [
  option("pok1leadership", `${RIVAL}'s Leadership`, "strategy card"),
  option("pok4construction", `${THIRD}'s Construction`, "strategy card"),
  option("pok7technology", "Technology", "strategy card"),
];
const holders = { pok1leadership: RIVAL, pok4construction: THIRD, pok7technology: null };
export const overrule: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "Overrule: which card's primary ability to perform",
    context: { subtype: "overrule_pick_strategy card", source: { ActionCard: "overrule" } },
    options: strategyOptions(),
    ...(legacy ? {} : { details: { holders } }),
  },
};
export const strategize: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "Strategize: which card's secondary ability to perform",
    context: { subtype: "strategize2_pick_strategy card", source: { ActionCard: "strategize2" } },
    options: strategyOptions(),
    ...(legacy ? {} : { details: { holders } }),
  },
};

export const repeal: Scene = {
  ready: "pending-choice-dialog",
  game: { table: { laws: { checks: "against", minister_commerce: RIVAL, conventions: "for" } } },
  choice: {
    prompt: "repeal which law",
    context: { subtype: "repeal_pick_repeal", source: { ActionCard: "repeal" } },
    options: ["checks", "conventions", "minister_commerce"].map((id) => option(id, id, "repeal")),
    ...(legacy
      ? {}
      : {
          details: {
            laws: {
              checks: printed("checks", "against"),
              conventions: printed("conventions", "for"),
              minister_commerce: printed("minister_commerce", RIVAL),
            },
          },
        }),
  },
};

const dice = [
  { unit: "combat value 8", unit_index: 0, die: 0, face: 3, bonus: 0, hits_on: 8, rerolled: false },
  { unit: "combat value 8", unit_index: 0, die: 1, face: 9, bonus: 0, hits_on: 8, rerolled: false },
  { unit: "combat value 8", unit_index: 0, die: 2, face: 5, bonus: 0, hits_on: 8, rerolled: false },
  { unit: "combat value 6", unit_index: 1, die: 0, face: 7, bonus: 0, hits_on: 6, rerolled: false },
  { unit: "combat value 6", unit_index: 1, die: 1, face: 2, bonus: 0, hits_on: 6, rerolled: false },
];
export const fireTeam: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "reroll die 1 of combat value 8",
    context: { subtype: "fire_team_reroll", source: { ActionCard: "fire_team" }, optional: true, target: { System: "18" } },
    options: [
      option("reroll|0:0", "reroll die 1 of combat value 8 (shows 3)", "reroll_die", { unit: "combat value 8", face: 3, hits_on: 8 }),
      option("decline", "Decline", "decline"),
    ],
    ...(legacy ? {} : { details: { dice, current: 0 } }),
  },
};

// Insider Information leaves no decision: the player who looked sees the top of the deck in their
// own view (PlayerView.agenda_peek). The before shot is the same game state on code that ignores it.
export const insider: Scene = {
  hold: 1200,
  choice: null,
  game: {
    phase: "agenda",
    players: players({
      agenda_peek: [printed("minister_commerce"), printed("mutiny"), printed("secret")].map((card) => ({
        id: card.id,
        name: card.name,
        type: card.type,
        target: card.target,
        text1: card.text1,
        text2: card.text2,
      })),
    }),
  },
};

const upgradeBoard = () =>
  sceneBoard({
    "19": {
      command_tokens: [],
      units: [
        { unit_type: "cruiser", owner: ME, damaged: false },
        { unit_type: "cruiser2", owner: ME, damaged: true },
        { unit_type: "carrier", owner: ME, damaged: false },
      ],
    },
  });
export const upgrade: Scene = {
  ready: "pending-choice-dialog",
  game: { board: { ...upgradeBoard(), active_system: "19" } },
  choice: {
    prompt: "Upgrade: which cruiser becomes a dreadnought",
    context: { subtype: "upgrade_pick_cruiser", source: { ActionCard: "upgrade" }, target: { System: "19" } },
    options: [option("0", "cruiser", "cruiser", legacy ? undefined : { system: "19" }), option("1", "cruiser2", "cruiser", legacy ? undefined : { system: "19" })],
    ...(legacy
      ? {}
      : {
          details: {
            units: {
              "0": { system: "19", planet: null, unit: "cruiser", owner: ME, damaged: false, cost: 2 },
              "1": { system: "19", planet: null, unit: "cruiser2", owner: ME, damaged: true, cost: 2 },
            },
          },
        }),
  },
};

const plagueBoard = () =>
  sceneBoard({
    "28": {
      units: [
        ...sceneBoard().systems["28"].units,
        { unit_type: "infantry", owner: RIVAL, planet: "tequran", damaged: false },
        { unit_type: "infantry", owner: RIVAL, planet: "tequran", damaged: false },
        { unit_type: "mech", owner: RIVAL, planet: "tequran", damaged: false },
        { unit_type: "infantry", owner: RIVAL, planet: "torkan", damaged: false },
      ],
    },
  });
export const plague: Scene = {
  ready: "planet-selection-bar",
  game: { board: plagueBoard() },
  choice: {
    prompt: "Plague: which planet",
    context: { subtype: "plague_pick_planet", source: { ActionCard: "plague" } },
    options: [
      option("28|tequran", "tequran", "planet", { planet: "tequran", system: "28" }),
      option("28|torkan", "torkan", "planet", { planet: "torkan", system: "28" }),
    ],
  },
  after: async (page) => {
    await page.getByTestId("planet-chip-tequran").click();
  },
};

export const rescue: Scene = {
  ready: "pending-choice-dialog",
  game: {
    board: {
      ...sceneBoard({
        "24": { units: [{ unit_type: "cruiser", owner: ME, damaged: false }, { unit_type: "cruiser", owner: ME, damaged: false }] },
        "20": { units: [{ unit_type: "destroyer", owner: ME, damaged: false }, { unit_type: "carrier", owner: ME, damaged: false }] },
      }),
      active_system: "26",
    },
  },
  choice: {
    prompt: "Rescue: bring a ship from where",
    context: { subtype: "rescue_pick_rescue_from", source: { ActionCard: "rescue" }, target: { System: "26" } },
    options: [
      option("24", "a ship from 24", "rescue_from", legacy ? undefined : { system: "24" }),
      option("20", "a ship from 20", "rescue_from", legacy ? undefined : { system: "20" }),
    ],
  },
};

// ---- player picks with facts ------------------------------------------------------------------

const traitBoard = (): BoardView => {
  const board = sceneBoard();
  board.map_tiles = board.map_tiles!.map((tile) =>
    tile.system_id === "28"
      ? { ...tile, planets: tile.planets!.map((planet) => ({ ...planet, traits: planet.id === "tequran" ? ["cultural"] : ["hazardous"] })) }
      : tile.system_id === "18"
        ? { ...tile, planets: tile.planets!.map((planet) => ({ ...planet, traits: ["cultural"] })) }
        : tile,
  );
  board.systems["28"] = {
    ...board.systems["28"],
    planets: {
      tequran: { planet_id: "tequran", controlled_by: RIVAL, exhausted: true },
      torkan: { planet_id: "torkan", controlled_by: RIVAL, exhausted: false },
    },
  };
  board.systems["27"] = {
    ...board.systems["27"],
    planets: {
      new_albion: { planet_id: "new_albion", controlled_by: THIRD, exhausted: false },
      starpoint: { planet_id: "starpoint", controlled_by: THIRD, exhausted: false },
    },
  };
  return board;
};
const playerPick = (alias: string, prompt: string, label: (id: string) => string): Scene => ({
  ready: "pending-choice-dialog",
  game: {
    board: traitBoard(),
    players: players({ fleet_tokens: 2 }, { relic_fragments: { cultural: 2, hazardous: 1 }, tactic_tokens: 3, fleet_tokens: 4 }, { tactic_tokens: 0, fleet_tokens: 1 }),
  },
  choice: {
    prompt,
    context: { subtype: `${alias}_pick_player`, source: { ActionCard: alias } },
    options: [RIVAL, THIRD].map((id) => option(id, label(id), "player")),
  },
});
export const abs = playerPick("abs", "Ancient Burial Sites: choose 1 player", (id) => `target ${id}`);
export const insub = playerPick("insub", "Insubordination: whose tactic pool", (id) => `take a tactic token from ${id}`);
export const seizePlayer = playerPick("seize", "Seize Artifact: which neighbor to take from", (id) => id);
export const fsb = playerPick("fsb", "Forward Supply Base: choose another player", (id) => `pay ${id}`);

// ---- Ghost Squad, Lie in Wait, Seize fragment, Bribery ----------------------------------------

const ghostBoard = () =>
  sceneBoard({
    "18": {
      ...sceneBoard().systems["18"],
      units: [
        ...Array.from({ length: 3 }, () => ({ unit_type: "infantry", owner: ME, planet: "jord", damaged: false })),
        { unit_type: "mech", owner: ME, planet: "jord", damaged: false },
        { unit_type: "infantry", owner: ME, planet: "exhausted", damaged: false },
        { unit_type: "carrier", owner: ME, damaged: false },
      ],
    },
  });
const ghostMove = (from: string, to: string, unit: string, count: number) =>
  option(`move|${from}|${to}|${unit}`, `move ${count} ${unit} from ${from} to ${to}`, "move", legacy ? undefined : { from, to, unit, count });
export const ghostSquad: Scene = {
  ready: "pending-choice-dialog",
  game: { board: { ...ghostBoard(), active_system: "18" } },
  choice: {
    prompt: "Ghost Squad: move ground forces in 18",
    context: { subtype: "ghost_squad_move", source: { ActionCard: "ghost_squad" }, target: { System: "18" }, optional: true },
    options: [
      ghostMove("jord", "exhausted", "infantry", 3),
      ghostMove("jord", "exhausted", "mech", 1),
      ghostMove("exhausted", "jord", "infantry", 1),
      option("decline", "Decline", "decline"),
    ],
  },
};

export const lieInWait: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "Lie in Wait: take which action card",
    context: { subtype: "lieinwait_pick_action_card", source: { ActionCard: "lieinwait" } },
    options: [
      option("spy", `take spy from ${RIVAL}`, "action_card"),
      option("direct_hit", `take direct_hit from ${RIVAL}`, "action_card"),
      option("bribery", `take bribery from ${RIVAL}`, "action_card"),
    ],
    ...(legacy ? {} : { details: { from: RIVAL } }),
  },
};

export const seizeFragment: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "Seize Artifact: which fragment to take",
    context: { subtype: "seize_pick_fragment", source: { ActionCard: "seize" } },
    options: [option("cultural", "cultural", "fragment"), option("hazardous", "hazardous", "fragment")],
  },
};

export const bribery: Scene = {
  ready: "pending-choice-dialog",
  choice: {
    prompt: "Bribery: how many trade goods to spend?",
    context: { subtype: "bribery_pick_count", source: { ActionCard: "bribery" } },
    options: [0, 1, 2, 3, 4].map((n) => option(String(n), `spend ${n} trade goods`, "count")),
  },
};
