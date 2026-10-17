import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { pageQuery, spaceOdds } from "@ti4/shared/engine/odds.ts";
import { describe, expect, it } from "vitest";
import battleUpdate from "../../../fixtures/protocol/update-battle.json";
import movementUpdate from "../../../fixtures/protocol/update-movement.json";
import { type MovementDraft, setLoad, setShip } from "./movementDraft";
import { type KeptOdds, oddsRequest } from "./movementOdds";
import { percentages, selectArrival, selectMovement } from "./select/movement";
import type { BattleResponse, MovementFacts, SessionUpdate } from "./wire";

// An update of a real game, written by the engine: in the late game, seat a (Sol) moves into
// the system of seat c (Letnev), which has two cruisers, a destroyer and a PDS there
// (`crates/ti4-wasm/tests/it/protocol_fixtures.rs` (`cargo wire-web`)).
const update = battleUpdate as unknown as SessionUpdate;
const facts = update.tactical as MovementFacts;
const WARSUN = "01|1";
const CARRIER = "01|2";
const FIGHTERS = facts.cargo.findIndex((pool) => pool.system === "01" && pool.unit === "fighter2");
const INFANTRY = facts.cargo.findIndex(
  (pool) => pool.source === "jord" && pool.unit === "sol_infantry2",
);

const staged = (...changes: ((draft: MovementDraft) => MovementDraft)[]) =>
  changes.reduce((draft, change) => change(draft), {} as MovementDraft);
const ship = (id: string) => (draft: MovementDraft) => setShip(facts, draft, id, true);
const load = (id: string, pool: number, count: number) => (draft: MovementDraft) =>
  setLoad(facts, draft, id, pool, count);

/** An answer of the engine, as it could be: Sol wins mostly, and keeps most of its ships. */
const RESPONSE: BattleResponse = {
  simulations: 2000,
  attacker_win_rate: 0.7234,
  defender_win_rate: 0.2496,
  mutual_destruction_rate: 0.025,
  unresolved_rate: 0.002,
  average_rounds: 2.1,
  attacker_expected_survivors: { warsun: 0.9, fighter2: 1.84 },
  defender_expected_survivors: { cruiser2: 0.31, cruiser: 0.2, destroyer: 0.05 },
  attacker_fielded: { warsun: 1, fighter2: 2 },
  defender_fielded: { cruiser: 2, destroyer: 1 },
};

it("has the battle of the engine in the facts", () => {
  expect(facts.active).toBe("32");
  expect(facts.battle).toEqual({
    defender: "c",
    attacker: { faction: "sol" },
    defender_side: {
      faction: "letnev",
      units: { cruiser: 2, destroyer: 1 },
      guns: { pds: 1 },
    },
  });
  expect(facts.ships.find((item) => `${item.origin}|${item.index}` === WARSUN)?.unit).toBe(
    "warsun",
  );
  expect(FIGHTERS).toBeGreaterThanOrEqual(0);
  expect(INFANTRY).toBeGreaterThanOrEqual(0);
});

it("asks nothing when no ship moves in, or when no other seat has ships there", () => {
  expect(oddsRequest(facts, {})).toBeNull();
  const peaceful = movementUpdate.tactical as unknown as MovementFacts;
  expect(peaceful.battle).toBeUndefined();
  expect(oddsRequest(peaceful, { [`${peaceful.ships[0].origin}|0`]: {} })).toBeNull();
});

it("adds the ships that move in and the fighters in their holds to the attacker", () => {
  const draft = staged(
    ship(WARSUN),
    load(WARSUN, FIGHTERS, 2),
    load(WARSUN, INFANTRY, 3),
    ship(CARRIER),
  );
  const asked = oddsRequest(facts, draft);
  // Infantry does not fight in space; the space cannon of the defender fires first.
  expect(asked?.request).toEqual({
    attacker: { faction: "sol", units: { fighter2: 2, sol_carrier2: 1, warsun: 1 } },
    defender: { faction: "letnev", units: { cruiser: 2, destroyer: 1 }, guns: { pds: 1 } },
  });
  expect(asked?.key).toBe(JSON.stringify(asked?.request));
  // The same fleet staged in another order is the same question.
  const again = staged(
    ship(CARRIER),
    ship(WARSUN),
    load(WARSUN, INFANTRY, 3),
    load(WARSUN, FIGHTERS, 2),
  );
  expect(oddsRequest(facts, again)?.key).toBe(asked?.key);
  expect(oddsRequest(facts, staged(ship(WARSUN)))?.key).not.toBe(asked?.key);
});

it("rounds the rates to whole percentages that sum to 100", () => {
  expect(percentages([0.7234, 0.027, 0.2496])).toEqual([72, 3, 25]);
  expect(percentages([1 / 3, 1 / 3, 1 / 3])).toEqual([34, 33, 33]);
  expect(percentages([0.005, 0.005, 0.99])).toEqual([1, 0, 99]);
  expect(percentages([1, 0, 0])).toEqual([100, 0, 0]);
});

describe("the odds in the movement", () => {
  const draft = staged(ship(WARSUN), load(WARSUN, FIGHTERS, 2));
  const asked = oddsRequest(facts, draft)!;
  const view = (kept: KeptOdds | null) => selectMovement(update, facts, draft, [], kept).odds;

  it("is pending before the first answer, with the ships that fight", () => {
    expect(view(null)).toEqual({
      defender: "c",
      win: null,
      draw: null,
      loss: null,
      rollouts: 0,
      pending: true,
      sides: [
        {
          seat: "a",
          ships: [
            { unit: "warsun", expected: null, fielded: 1 },
            { unit: "fighter", expected: null, fielded: 2 },
          ],
        },
        {
          seat: "c",
          ships: [
            { unit: "cruiser", expected: null, fielded: 2 },
            { unit: "destroyer", expected: null, fielded: 1 },
          ],
        },
      ],
    });
    expect(selectMovement(update, facts, {}, [], null).odds).toBeNull();
  });

  it("has Win, Draw and Loss and the survivors by kind of the answer", () => {
    const odds = view({ key: asked.key, response: RESPONSE });
    expect(odds).toMatchObject({ win: 72, draw: 3, loss: 25, rollouts: 2000, pending: false });
    expect(odds?.sides.map((side) => side.ships)).toEqual([
      [
        { unit: "warsun", expected: "0.9", fielded: 1 },
        { unit: "fighter", expected: "1.8", fielded: 2 },
      ],
      [
        // Two ids of one kind are one kind.
        { unit: "cruiser", expected: "0.5", fielded: 2 },
        { unit: "destroyer", expected: "0.1", fielded: 1 },
      ],
    ]);
    // The active system has the same odds in its inspector.
    expect(
      selectArrival(update, facts, draft, { key: asked.key, response: RESPONSE }).odds,
    ).toEqual(odds);
  });

  it("keeps the numbers of another movement as pending, with the ships of this one", () => {
    const odds = view({ key: "another movement", response: RESPONSE });
    expect(odds).toMatchObject({ win: 72, draw: 3, loss: 25, pending: true });
    expect(odds?.sides[0].ships).toEqual([
      { unit: "warsun", expected: "0.9", fielded: 1 },
      { unit: "fighter", expected: "1.8", fielded: 2 },
    ]);
  });

  it("shows nothing when the engine had no odds for these sides", () => {
    expect(view({ key: asked.key, response: null })).toBeNull();
  });
});

const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));

describe.skipIf(!existsSync(WASM))("the odds of the engine in this page", () => {
  it("computes the odds of the staged movement, the same for the same sides", async () => {
    const query = pageQuery(await WebAssembly.compile(readFileSync(WASM)));
    const draft = staged(ship(WARSUN), load(WARSUN, FIGHTERS, 3), ship(CARRIER));
    const asked = oddsRequest(facts, draft)!;
    const response = await spaceOdds(query, asked.request);
    expect(response.attacker_fielded).toMatchObject({ warsun: 1, sol_carrier2: 1, fighter2: 3 });
    expect(response.defender_fielded).toMatchObject({ cruiser: 2, destroyer: 1 });
    expect(await spaceOdds(query, asked.request)).toEqual(response);
    const odds = selectMovement(update, facts, draft, [], { key: asked.key, response }).odds!;
    expect(odds.win! + odds.draw! + odds.loss!).toBe(100);
    // A war sun, a carrier and three fighters against two cruisers and a destroyer.
    expect(odds.win).toBeGreaterThan(odds.loss!);
  });
});
