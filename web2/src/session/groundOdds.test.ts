import { expect, it } from "vitest";
import landingUpdate from "../../../fixtures/protocol/update-landing.json";
import { groundAsks } from "./groundOdds";
import { selectLanding } from "./select/invasion";
import type { InvasionFacts, InvasionOddsContext, SessionUpdate } from "./wire";

// The landings of `update-landing` (seat a, Sol, at the free planet Saudor of 23), with Letnev's
// ground forces and a PDS put on Saudor, and the context of the odds that the engine sends.
const fixture = landingUpdate as unknown as SessionUpdate;
const facts = fixture.tactical as InvasionFacts;

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

it("shows the last answer, pending while the answer for new landings is on its way", () => {
  const update = defended();
  const [ask] = groundAsks(update, facts, { saudor: { 0: 1 } });
  const key = ask.kind === "ask" ? ask.ask.key : "";
  const odds = (draft: Record<number, number>, kept = {}) =>
    selectLanding(update, facts, { saudor: draft }, null, null, kept).planets[0].odds;
  expect(odds({ 0: 1 })).toEqual({
    defender: "c",
    win: null,
    rollouts: 0,
    pending: true,
    unavailable: false,
  });
  const answered = { saudor: { key, response: { simulations: 2000, attacker_win_rate: 0.346 } } };
  expect(odds({ 0: 1 }, answered)).toMatchObject({ win: 35, rollouts: 2000, pending: false });
  expect(odds({ 0: 2 }, answered)).toMatchObject({ win: 35, pending: true });
  // The engine had no odds for these sides.
  expect(odds({ 0: 1 }, { saudor: { key, response: null } })).toBeNull();
  // Nothing lands: no odds.
  expect(selectLanding(update, facts, {}, null).planets[0].odds).toBeNull();
});

it("says that there are no odds for a defense that they cannot count", () => {
  const update = defended({ available: false });
  expect(selectLanding(update, facts, { saudor: { 0: 1 } }, null).planets[0].odds).toEqual({
    defender: "c",
    win: null,
    rollouts: 0,
    pending: false,
    unavailable: true,
  });
});
