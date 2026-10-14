import type { Page, TestInfo } from "@playwright/test";
import { openMockedGame, type MockChoice } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, galleryBoard } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";
import type { BoardView, InvasionStepView, PlacedUnitView } from "../../../src/protocol/types";

// TI4_SHOT_PREFIX=before is how the "before" shots were taken (on the base commit, 7f922896);
// the default "after" regenerates the current layout.
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const viewports = [
  { id: "desktop", width: 1440, height: 900 },
  { id: "phone", width: 390, height: 844 },
];

/** The defender's seat id as the engine sends it: a long participant id, not a name. */
export const RIVAL = `player_${"b793e54282b7a62cf6b1d7876eb".padEnd(64, "0")}`;
/** A third player with ground forces on the same planet (coexistence). */
export const OTHER = `player_${"7b2a83550545c580c4b919fa03f".padEnd(64, "a")}`;

export const players = [playerWithHand(), { ...opponent, id: RIVAL }, { ...opponent, id: OTHER, faction: "letnev" }];

const ground = (type: string, owner: string, damaged = false): PlacedUnitView => ({
  unit_type: type,
  owner,
  damaged,
  planet: "jord",
});
const many = (n: number, type: string, owner: string) => Array.from({ length: n }, () => ground(type, owner));
const ships: PlacedUnitView[] = [
  { unit_type: "dreadnought", owner: actor, damaged: false },
  { unit_type: "dreadnought", owner: actor, damaged: false },
  { unit_type: "destroyer", owner: actor, damaged: false },
  { unit_type: "carrier", owner: actor, damaged: false },
];

/** Jord before anything happens: the rival's infantry, a mech and a PDS. */
export const defenders = [...many(3, "infantry", RIVAL), ground("mech", RIVAL), ground("pds", RIVAL)];
export const defendersAfterBombardment = [...many(1, "infantry", RIVAL), ground("mech", RIVAL, true), ground("pds", RIVAL)];
const landed = [...many(3, "infantry", actor), ground("mech", actor)];

const die = (player: string, group: string, face: number, target: number) => ({
  planet: "jord",
  player,
  group,
  face,
  target,
  hit: face >= target,
});

/** Two dreadnoughts (Bombardment 5, one die each) against the rival's forces: 6 and 9 hit, 2 misses. */
export const bombardmentStep: InvasionStepView = {
  planet: "jord",
  kind: "bombardment",
  round: 0,
  before: defenders,
  after: defendersAfterBombardment,
  dice: [die(actor, "dreadnought", 9, 5), die(actor, "dreadnought", 7, 5), die(actor, "dreadnought", 2, 5)],
  hits: { [actor]: 2 },
  harrow_hits: 0,
};

/** The rival's PDS (Space Cannon 6) fires at the landed forces: one hit, one infantry dies. */
export const spaceCannonStep: InvasionStepView = {
  planet: "jord",
  kind: "space_cannon_defense",
  round: 0,
  before: [...defendersAfterBombardment, ...landed],
  after: [...defendersAfterBombardment, ...many(2, "infantry", actor), ground("mech", actor)],
  dice: [die(RIVAL, "pds", 8, 6), die(RIVAL, "pds", 3, 6)],
  hits: { [RIVAL]: 1 },
  harrow_hits: 0,
};

/** The last ground round: the invader wipes out the defending ground forces. */
export const groundStep: InvasionStepView = {
  planet: "jord",
  kind: "ground_round",
  round: 2,
  before: [...many(1, "infantry", RIVAL), ground("mech", RIVAL, true), ground("pds", RIVAL), ...many(2, "infantry", actor), ground("mech", actor)],
  after: [ground("pds", RIVAL), ...many(2, "infantry", actor), ground("mech", actor)],
  dice: [],
  hits: { [actor]: 3, [RIVAL]: 0 },
  harrow_hits: 0,
};

export function invasionBoard(opts: {
  phase: string;
  units: PlacedUnitView[];
  step?: InvasionStepView | null;
  controller?: string | null;
  invasion?: boolean;
}): BoardView {
  const base = galleryBoard.systems["18"];
  return {
    ...galleryBoard,
    combat: null,
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...base,
        planets: {
          jord: { planet_id: "jord", controlled_by: opts.controller === undefined ? RIVAL : opts.controller, exhausted: false },
        },
        units: [...ships, ...opts.units],
      },
    },
    invasion:
      opts.invasion === false
        ? null
        : {
            system_id: "18",
            invasion_seq: 7,
            invader: actor,
            phase: opts.phase,
            planets: ["jord"],
            current_planet: "jord",
            defender: RIVAL,
            ground_round: opts.step?.kind === "ground_round" ? opts.step.round : 0,
            last_step: opts.step ?? null,
            odds_context: {},
          },
  };
}

export async function openInvasion(
  page: Page,
  viewport: (typeof viewports)[number],
  board: BoardView,
  choice: MockChoice | null,
) {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  const game = await openMockedGame(page, { players, board: board as never, choice });
  await page.waitForTimeout(800);
  return game;
}

export async function take(page: Page, testInfo: TestInfo, id: string, viewport: (typeof viewports)[number]) {
  await shot(page, testInfo, `${prefix}-${id}-${viewport.id}`);
}

/** What the engine sends for the coexistence bombardment question (details are the new display-only part). */
export const bombardmentTargetChoice: MockChoice = {
  prompt: "whose units on jord take the bombardment's next hits (2 hits)",
  context: { subtype: "bombardment_target", target: { Planet: { system: "18", planet: "jord" } } },
  options: [
    { id: RIVAL, label: `${RIVAL}'s units`, kind: "bombardment_target", payload: { system: "18", planet: "jord" } },
    { id: OTHER, label: `${OTHER}'s units`, kind: "bombardment_target", payload: { system: "18", planet: "jord" } },
  ] as never,
  details: {
    kind: "offer",
    card: {
      title: "Bombardment hits",
      tag: "bombardment",
      window: "Invasion, step 1: bombardment",
      text: "Hits only destroy ground forces (never structures); a Mech may sustain damage to cancel one. Several players have ground forces on this planet: choose whose take these hits.",
    },
    facts: [
      { label: "Planet", planet: "jord", system: "18" },
      { label: "Bombarding", seat: actor },
      { label: "Hits to assign", value: "2" },
    ],
    captions: {
      [RIVAL]: { label: "Hit their ground forces", hint: "3 infantry, 1 mech, 1 pds on jord", seat: RIVAL },
      [OTHER]: { label: "Hit their ground forces", hint: "2 infantry on jord", seat: OTHER },
    },
  },
};

export const fightChoice: MockChoice = {
  prompt: "ground combat on jord: roll round 1",
  context: { subtype: "fight_ground_combat_round", target: { Planet: { system: "18", planet: "jord" } } },
  options: [{ id: "fight", label: "Roll ground combat", kind: "fight_ground_combat_round" }] as never,
};

export { actor, galleryBoard };
export const withBombardTargets = [...defenders, ...many(2, "infantry", OTHER)];
export { landed };
