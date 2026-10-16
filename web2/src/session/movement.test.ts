import { expect, it } from "vitest";
import loadUpdate from "../../../fixtures/protocol/update-load.json";
import movementUpdate from "../../../fixtures/protocol/update-movement.json";
import {
  type MovementDraft,
  fill,
  poolLeft,
  resetOrigin,
  setCount,
  setLoad,
  setShip,
  loadKey,
  shipId,
  shipKey,
} from "./movementDraft";
import { type MovementStep, draftOf, stepOf, stepsOf } from "./movementPlan";
import { selectMovement, selectMovementBoard, selectOrigin } from "./select/movement";
import type { MovementFacts, SessionUpdate } from "./wire";

// Updates of a real game, written by the engine (seat a is Sol; it activated the system next
// to its home, where it has two carriers, a destroyer, three fighters and five infantry), from
// `crates/ti4-wasm/tests/it/protocol_fixtures.rs` (`cargo wire-web`).
const moving = movementUpdate as unknown as SessionUpdate;
const loading = loadUpdate as unknown as SessionUpdate;
const facts = moving.tactical as MovementFacts;
const FIGHTERS = facts.cargo.findIndex((pool) => pool.unit === "fighter");
const INFANTRY = facts.cargo.findIndex((pool) => pool.unit === "sol_infantry");
const [CARRIER, CARRIER_2, DESTROYER] = ["01|0", "01|1", "01|2"];

const staged = (...changes: ((draft: MovementDraft) => MovementDraft)[]) =>
  changes.reduce((draft, change) => change(draft), {} as MovementDraft);
const ship = (id: string) => (draft: MovementDraft) => setShip(facts, draft, id, true);
const load = (id: string, pool: number, count: number) => (draft: MovementDraft) =>
  setLoad(facts, draft, id, pool, count);

it("has the facts of the engine for the fixture", () => {
  expect(facts.active).toBe("23");
  expect(facts.ships.map((item) => [item.unit, !!item.move])).toEqual([
    ["sol_carrier", true],
    ["sol_carrier", true],
    ["destroyer", true],
  ]);
  expect(facts.cargo.map((pool) => [pool.unit, pool.source, pool.count])).toEqual([
    ["fighter", null, 3],
    ["sol_infantry", "jord", 5],
  ]);
});

it("loads a ship that moves, within its hold and what is left", () => {
  // A ship that stays has no hold.
  expect(load(CARRIER, INFANTRY, 2)({})).toEqual({});
  const draft = staged(ship(CARRIER), load(CARRIER, INFANTRY, 2));
  expect(draft).toEqual({ [CARRIER]: { [INFANTRY]: 2 } });
  expect(poolLeft(facts, draft, INFANTRY)).toBe(3);
  // The hold has six slots, and there are three fighters.
  const full = staged(ship(CARRIER), load(CARRIER, INFANTRY, 9), load(CARRIER, FIGHTERS, 9));
  expect(full[CARRIER]).toEqual({ [INFANTRY]: 5, [FIGHTERS]: 1 });
  // The other carrier gets what the first left.
  const both = staged(
    () => full,
    ship(CARRIER_2),
    load(CARRIER_2, INFANTRY, 2),
    (draft) => setLoad(facts, draft, CARRIER_2, FIGHTERS, 9),
  );
  expect(both[CARRIER_2]).toEqual({ [FIGHTERS]: 2 });
  // A destroyer has no hold.
  expect(staged(ship(DESTROYER), load(DESTROYER, FIGHTERS, 1))).toEqual({ [DESTROYER]: {} });
});

it("takes a ship back with its hold, and a system with all its ships", () => {
  const draft = staged(ship(CARRIER), load(CARRIER, INFANTRY, 2), ship(DESTROYER));
  expect(setShip(facts, draft, CARRIER, false)).toEqual({ [DESTROYER]: {} });
  expect(resetOrigin(draft, "01")).toEqual({});
  expect(resetOrigin(draft, "23")).toEqual(draft);
});

it("reads the intents of the sheet: a token, a slot, Fill", () => {
  let draft = setCount(facts, {}, shipKey(CARRIER), 1);
  draft = setCount(facts, draft, shipKey(CARRIER_2), 1);
  draft = setCount(facts, draft, loadKey(CARRIER, FIGHTERS), 1);
  expect(draft).toEqual({ [CARRIER]: { [FIGHTERS]: 1 }, [CARRIER_2]: {} });
  // Fill loads the kind on every ship that leaves, in order, until none is left.
  draft = fill(facts, draft, "01", INFANTRY);
  expect(draft).toEqual({ [CARRIER]: { [FIGHTERS]: 1, [INFANTRY]: 5 }, [CARRIER_2]: {} });
  draft = setCount(facts, draft, loadKey(CARRIER, INFANTRY), 4);
  draft = setCount(facts, draft, shipKey(CARRIER_2), 0);
  expect(draft).toEqual({ [CARRIER]: { [FIGHTERS]: 1, [INFANTRY]: 4 } });
  expect(setCount(facts, draft, "something else", 1)).toBe(draft);
});

const plan = () =>
  stepsOf(
    facts,
    staged(ship(CARRIER), load(CARRIER, INFANTRY, 2), load(CARRIER, FIGHTERS, 1), ship(DESTROYER)),
  );

it("turns a staged movement into steps, ship by ship", () => {
  expect(
    plan().map((step) => (step.kind === "move" || step.kind === "load" ? step.unit : step.kind)),
  ).toEqual([
    "sol_carrier",
    "fighter",
    "sol_infantry",
    "sol_infantry",
    "done_loading",
    "destroyer",
    "done_moving",
  ]);
  expect(plan()[2]).toEqual({
    kind: "load",
    origin: "01",
    pickup_system: "01",
    unit: "sol_infantry",
    source: "jord",
    damaged: false,
    galvanized: false,
  });
  // Moving nothing is a movement too.
  expect(stepsOf(facts, {})).toEqual([{ kind: "done_moving" }]);
});

it("stages the steps of a stopped plan again", () => {
  const draft = staged(ship(CARRIER), load(CARRIER, INFANTRY, 2), ship(CARRIER_2), ship(DESTROYER));
  expect(draftOf(facts, stepsOf(facts, draft))).toEqual(draft);
  // The rest of a plan: the first carrier has moved, so the second is the first that is left.
  const rest = stepsOf(facts, staged(ship(CARRIER_2), load(CARRIER_2, FIGHTERS, 2)));
  expect(draftOf(facts, rest)).toEqual({ [CARRIER]: { [FIGHTERS]: 2 } });
  // A step that no longer fits is left out.
  const gone: MovementStep[] = [
    {
      kind: "move",
      origin: "99",
      unit: "cruiser",
      damaged: false,
      gravity_drive: false,
      ionian: false,
    },
    { kind: "done_moving" },
  ];
  expect(draftOf(facts, gone)).toEqual({});
});

const moveChoice = moving.pending_choice!.choice;
const loadChoice = loading.pending_choice!.choice;

// How a step answers a choice is decided in the engine (`plan_answer` in ti4-host); the tests
// of it are there, and `wasmTransport.test.ts` sends plans to the real engine.
it("reads the step back from the option that answered it", () => {
  // Every option of a movement is a step, and no two options are the same step.
  for (const choice of [moveChoice, loadChoice]) {
    const steps = choice.options.map((option) => JSON.stringify(stepOf(option)));
    expect(steps).not.toContain("null");
    expect(new Set(steps).size).toBe(steps.length);
  }
  expect(stepOf({ id: "tactical", kind: "action", payload: {} })).toBeNull();
});

it("ties the row of each ship to its system and its own path on the map", () => {
  const origin = selectOrigin(moving, facts, staged(ship(CARRIER)), "01")!;
  const units = origin.ships.flatMap((row) => row.units);
  for (const unit of units) {
    const fact = facts.ships.find((item) => shipKey(shipId(item)) === unit.key);
    expect(unit.link).toEqual([`rt:${fact!.move!.path.join(">")}`, "sys:01"]);
  }
  expect(units).toHaveLength(3);
});

it("says what no ship can load, and why: in the card of an origin, or under the movement", () => {
  // The facts as the engine writes them for a wormhole on the path and cargo that stays.
  const more: MovementFacts = {
    ...facts,
    ships: facts.ships.map((item, index) =>
      index === 0 ? { ...item, move: { ...item.move!, wormholes: ["BETA"] } } : item,
    ),
    unloadable: [
      {
        system: "01",
        source: null,
        unit: "fighter",
        damaged: false,
        count: 2,
        reason: "no_capacity",
      },
      {
        system: "70",
        source: "kraag",
        unit: "sol_infantry",
        damaged: false,
        count: 1,
        reason: "command_token",
      },
    ],
  };
  const origin = selectOrigin(moving, more, {}, "01")!;
  expect(origin.warning).toBe(
    "Stay: 2 Fighters (Space area) · No ship with capacity starts or passes here",
  );
  expect(origin.ships[0].units[0].route).toContain("β wormhole");
  const view = selectMovement(moving, more, {});
  expect(view.unreachable?.rows).toEqual([
    {
      unit: "infantry",
      name: "1 Infantry · Planet Kraag",
      text: expect.stringContaining("· Your command token is here · no pickup"),
      system: "70",
    },
  ]);
  const board = selectMovementBoard(moving, more, {}, null);
  expect(board.notes["70"]).toEqual({
    sign: "cargo",
    text: "1 Infantry · Planet Kraag cannot be loaded · Your command token is here · no pickup",
  });
  expect(board.notes["01"]).toBeUndefined();
});
