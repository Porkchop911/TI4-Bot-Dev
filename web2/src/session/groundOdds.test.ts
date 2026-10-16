import { expect, it } from "vitest";
import landingUpdate from "../../../fixtures/protocol/update-landing.json";
import { groundAsks } from "./groundOdds";
import { selectLanding } from "./select/invasion";
import type {
  Catalog,
  GroundOddsResponse,
  InvasionFacts,
  InvasionOddsContext,
  SessionUpdate,
} from "./wire";

// The landings of `update-landing` (seat a, Sol, at the free planet Saudor of 23), with Letnev's
// ground forces and a PDS put on Saudor, and the context of the odds that the engine sends.
const fixture = landingUpdate as unknown as SessionUpdate;
const facts = fixture.tactical as InvasionFacts;

/** The unit cards of the combat, as the catalogue prints them. It has no `letnev_infantry`. */
const printed = {
  records: {
    units: [
      { id: "sol_infantry", base_type: "infantry", name: "Spec Ops I" },
      { id: "letnev_mech", base_type: "mech", name: "Dunlain Reaper" },
    ],
  },
} as unknown as Catalog;

function defended(context: Partial<InvasionOddsContext> = {}): SessionUpdate {
  const update = structuredClone(fixture);
  const board = update.view.board;
  const unit = (unit_type: string, damaged = false) => ({
    unit_type,
    owner: "c",
    planet: "saudor",
    damaged,
    galvanized: false,
  });
  board.systems["23"].units.push(
    unit("letnev_infantry"),
    unit("letnev_infantry"),
    unit("letnev_mech", true),
    unit("pds"),
  );
  board.invasion!.odds_context = {
    saudor: {
      opponent: "c",
      available: true,
      ground_force_types: ["letnev_infantry", "letnev_mech"],
      additional_guns: { pds: 1 },
      harrow_units: {},
      ...context,
    },
  };
  return update;
}

it("asks for the odds of a planet where units land against another seat", () => {
  const update = defended();
  expect(groundAsks(update, facts, {})).toEqual([]);
  const [ask] = groundAsks(update, facts, { saudor: { 0: 2 } });
  expect(ask).toMatchObject({ planet: "saudor", kind: "ask" });
  expect(ask.kind === "ask" && ask.ask).toEqual({
    planet: "saudor",
    defender: "c",
    key: JSON.stringify(ask.kind === "ask" && ask.ask.request),
    request: {
      attacker: { faction: "sol", units: { sol_infantry: 2 } },
      defender: {
        faction: "letnev",
        units: { letnev_infantry: 2, letnev_mech: 1 },
        damaged: { letnev_mech: 1 },
        guns: { pds: 1 },
      },
    },
  });
  // A planet that nobody defends has no ground combat.
  expect(groundAsks(fixture, facts, { saudor: { 0: 2 } })).toEqual([]);
});

/** An answer of `ti4-calc` for `request`'s sides (seat a lands Sol infantry on Saudor). */
function answer(attacker: number): GroundOddsResponse {
  return {
    simulations: 2000,
    attacker_win_rate: 0.346,
    defender_win_rate: 0.6,
    mutual_destruction_rate: 0.054,
    unresolved_rate: 0,
    average_rounds: 2.44,
    attacker_expected_survivors: { sol_infantry: 0.42 },
    defender_expected_survivors: { letnev_infantry: 0.9, letnev_mech: 0.75 },
    attacker_fielded: { sol_infantry: attacker },
    defender_fielded: { letnev_infantry: 2, letnev_mech: 1 },
    attacker_rolls: { sol_infantry: { hits_on: 7, dice: 1, modifier: 0 } },
    defender_rolls: {
      letnev_infantry: { hits_on: 8, dice: 1, modifier: 0 },
      letnev_mech: { hits_on: 6, dice: 1, modifier: 0 },
    },
  };
}

it("shows the last answer, pending while the answer for new landings is on its way", () => {
  const update = defended();
  const [ask] = groundAsks(update, facts, { saudor: { 0: 1 } });
  const key = ask.kind === "ask" ? ask.ask.key : "";
  const odds = (draft: Record<number, number>, kept = {}) =>
    selectLanding(update, facts, { saudor: draft }, null, null, kept, printed).planets[0].odds;
  expect(odds({ 0: 1 })).toEqual({
    defender: "c",
    win: null,
    draw: null,
    loss: null,
    rollouts: 0,
    pending: true,
    sides: [
      { seat: "a", units: [{ unit: "infantry", name: "Spec Ops I", expected: null, fielded: 1 }] },
      {
        seat: "c",
        units: [
          { unit: "mech", name: "Dunlain Reaper", expected: null, fielded: 1 },
          // A unit that the catalogue does not have is named by its base unit.
          { unit: "infantry", name: "Infantry", expected: null, fielded: 2 },
        ],
      },
    ],
    unavailable: false,
    preview: null,
  });
  const answered = { saudor: { key, response: answer(1) } };
  expect(odds({ 0: 1 }, answered)).toMatchObject({
    win: 35,
    draw: 5,
    loss: 60,
    rollouts: 2000,
    pending: false,
    sides: [
      { seat: "a", units: [{ unit: "infantry", expected: "0.4", fielded: 1 }] },
      {
        seat: "c",
        units: [
          { unit: "mech", expected: "0.8", fielded: 1 },
          { unit: "infantry", expected: "0.9", fielded: 2 },
        ],
      },
    ],
  });
  // New landings: the numbers of the last answer, pending, with the units that fight now.
  const pending = odds({ 0: 2 }, answered);
  expect(pending).toMatchObject({ win: 35, pending: true });
  expect(pending?.sides[0].units).toEqual([
    { unit: "infantry", name: "Spec Ops I", expected: "0.4", fielded: 2 },
  ]);
  // The engine had no odds for these sides.
  expect(odds({ 0: 1 }, { saudor: { key, response: null } })).toBeNull();
  // Nothing lands: no odds.
  expect(selectLanding(update, facts, {}, null).planets[0].odds).toBeNull();
});

it("has the first round of the ground combat, with the odds in the headers", () => {
  const update = defended();
  const [ask] = groundAsks(update, facts, { saudor: { 0: 2 } });
  const key = ask.kind === "ask" ? ask.ask.key : "";
  const odds = selectLanding(
    update,
    facts,
    { saudor: { 0: 2 } },
    null,
    null,
    { saudor: { key, response: answer(2) } },
    printed,
  ).planets[0].odds;
  const preview = odds?.preview;
  expect(preview?.table.label).toMatch(/· Ground combat · Round 1$/);
  expect(preview?.table.sides.map((side) => [side.seat, side.role])).toEqual([
    ["a", "Attacker"],
    ["c", "Defender"],
  ]);
  expect(
    preview?.table.sides.map((side) =>
      side.rows.map(({ unit, label, count, target, dice, hits }) => ({
        unit,
        label,
        count,
        target,
        dice,
        hits,
      })),
    ),
  ).toEqual([
    [{ unit: "infantry", label: "Spec Ops I", count: 2, target: 7, dice: 2, hits: "≈0.8" }],
    [
      { unit: "mech", label: "Dunlain Reaper", count: 1, target: 6, dice: 1, hits: "≈0.5" },
      { unit: "infantry", label: "Infantry", count: 2, target: 8, dice: 2, hits: "≈0.6" },
    ],
  ]);
  expect(preview?.odds).toMatchObject({
    kind: "ground",
    attacker: "a",
    defender: "c",
    attackerWins: 35,
    defenderWins: 60,
    rollouts: 2000,
    rounds: "2.4",
    attackerLeft: "0.4 of 2",
    defenderLeft: "1.6 of 3",
  });
});

it("marks a roll that the faction of a side changes", () => {
  const update = defended();
  const [ask] = groundAsks(update, facts, { saudor: { 0: 1 } });
  const key = ask.kind === "ask" ? ask.ask.key : "";
  const response = {
    ...answer(1),
    attacker_rolls: { sol_infantry: { hits_on: 7, dice: 1, modifier: -1 } },
  };
  const [row] = selectLanding(update, facts, { saudor: { 0: 1 } }, null, null, {
    saudor: { key, response },
  }).planets[0].odds!.preview!.table.sides[0].rows;
  expect(row).toMatchObject({
    target: 8,
    modified: true,
    targetTitle: "Printed 7+, −1 to each die",
  });
});

it("says that there are no odds for a defense that they cannot count", () => {
  const update = defended({ available: false });
  expect(selectLanding(update, facts, { saudor: { 0: 1 } }, null).planets[0].odds).toEqual({
    defender: "c",
    win: null,
    draw: null,
    loss: null,
    rollouts: 0,
    pending: false,
    sides: [],
    unavailable: true,
    preview: null,
  });
});
