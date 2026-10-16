import { expect, it } from "vitest";
import activationUpdate from "../../../../fixtures/protocol/update-activation.json";
import loadUpdate from "../../../../fixtures/protocol/update-load.json";
import movementUpdate from "../../../../fixtures/protocol/update-movement.json";
import { setLoad, setShip } from "../movementDraft";
import type { Catalog, SessionUpdate, TacticalFacts } from "../wire";
import type { LocalState } from "./action";
import { selectMovement } from "./movement";
import { selectShell } from "./shell";

// Updates of a real game, written by the engine: the activation, the movement and the loading
// of one tactical action (`crates/ti4-wasm/tests/it/protocol_fixtures.rs` (`cargo wire-web`)).
const activation = activationUpdate as unknown as SessionUpdate;
const movement = movementUpdate as unknown as SessionUpdate;
const facts = movement.tactical as Extract<TacticalFacts, { kind: "movement" }>;
const local = (change: Partial<LocalState> = {}): LocalState => ({
  inspected: null,
  staged: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: { builds: [], planets: [], goods: 0 },
  productionStage: "build",
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
  ...change,
});
/** The unit cards of the fixtures, as the catalogue prints them. */
const printed = {
  records: {
    units: [
      { id: "sol_carrier", base_type: "carrier", name: "Advanced Carrier I" },
      { id: "carrier", base_type: "carrier", name: "Carrier I" },
      { id: "cruiser", base_type: "cruiser", name: "Cruiser I" },
      { id: "destroyer", base_type: "destroyer", name: "Destroyer I" },
      { id: "dreadnought", base_type: "dreadnought", name: "Dreadnought I" },
      { id: "l1z1x_dreadnought", base_type: "dreadnought", name: "Super Dreadnought I" },
      { id: "fighter", base_type: "fighter", name: "Fighter I" },
      { id: "infantry", base_type: "infantry", name: "Infantry I" },
      { id: "sol_infantry", base_type: "infantry", name: "Spec Ops I" },
      { id: "pds", base_type: "pds", name: "PDS I" },
      { id: "spacedock", base_type: "spacedock", name: "Space Dock I" },
    ],
  },
} as unknown as Catalog;

const tactical = (update: SessionUpdate, state: LocalState) => {
  const { action } = selectShell(update, state);
  if (action.kind !== "tactical") {
    throw new Error("the choice has no screen of a tactical action");
  }
  return action;
};
const labels = (update: SessionUpdate, state: LocalState) =>
  tactical(update, state).task.footer.actions.map((item) => item.label);

/** One carrier with two infantry and a fighter, and the destroyer. */
const INFANTRY = facts.cargo.findIndex((pool) => pool.unit === "sol_infantry");
const FIGHTERS = facts.cargo.findIndex((pool) => pool.unit === "fighter");
let fleet = setShip(facts, {}, "01|0", true);
fleet = setLoad(facts, fleet, "01|0", INFANTRY, 2);
fleet = setLoad(facts, fleet, "01|0", FIGHTERS, 1);
fleet = setShip(facts, fleet, "01|2", true);

it("opens the activation step: the map is the picker, the panel has the choice", () => {
  const choice = activation.pending_choice!.choice;
  const view = tactical(activation, local());
  expect(view.tabs.map((tab) => tab.status)).toEqual([
    "decision",
    "future",
    "future",
    "future",
    "future",
  ]);
  expect(view.task.content).toMatchObject({
    kind: "activation",
    editing: true,
    chosen: null,
    tacticPool: [3, 2],
  });
  expect(labels(activation, local())).toEqual(["Activate system"]);
  expect(view.task.footer.actions[0].disabled).toBe(true);
  // Every system that can be activated is a target on the board.
  const { board } = selectShell(activation, local());
  expect(board.task).toMatchObject({ target: "system", kind: "pick", verb: "Activate" });
  expect(Object.keys(board.task!.values).sort()).toEqual(
    choice.options.map((option) => option.id).sort(),
  );
  // Only the systems where the action does something stand out: 11 in reach of the ships, and
  // home, where the space dock is. "Fit task" frames these.
  const highlight = board.task!.highlight!;
  expect(highlight).toHaveLength(12);
  expect(highlight).toContain("23");
  expect(highlight).toContain("01");
  expect(highlight.every((id) => id in board.task!.values)).toBe(true);
  expect(board.taskSystems).toEqual(highlight);
});

it("says how many ships are in range of the staged system", () => {
  const staged = local({ staged: "23", canUndo: true });
  const view = tactical(activation, staged);
  if (view.task.content.kind !== "activation") {
    throw new Error("not the activation step");
  }
  expect(view.task.content.chosen).toMatchObject({
    system: "23",
    shipsInRange: 3,
    originsInRange: 1,
    commandToken: false,
  });
  expect(view.context).toContain("#23");
  expect(labels(activation, staged)).toEqual(["Undo", "Clear", "Activate system"]);
  expect(view.task.footer.actions.at(-1)).toMatchObject({
    disabled: false,
    key: "Enter",
    intent: { type: "commitEdit" },
  });
  expect(selectShell(activation, staged).board.task!.chosen).toEqual({ "23": true });
});

it("opens the movement step with the fleet of each system that ships can leave", () => {
  const view = tactical(movement, local());
  expect(view.tabs.map((tab) => [tab.status, tab.caption])).toEqual([
    ["done", "#23"],
    ["decision", ""],
    ["future", ""],
    ["future", ""],
    ["future", ""],
  ]);
  const content = view.task.content;
  if (content.kind !== "movement") {
    throw new Error("not the movement step");
  }
  expect(content.target.system).toBe("23");
  expect(content.origins.map((origin) => [origin.system, origin.away, origin.leaves])).toEqual([
    ["01", 1, ""],
  ]);
  expect(content.origins[0].ships.map((ship) => [ship.name, ship.count, ship.total])).toEqual([
    ["Carrier", 0, 2],
    ["Destroyer", 0, 1],
  ]);
  expect(content.origins[0].cargo.map((item) => [item.name, item.place, item.left])).toEqual([
    ["Fighter", "Space area", 3],
    ["Infantry", "Planet Jord", 5],
  ]);
  expect(content.arrival).toEqual([]);
  expect(content.gauges).toEqual([{ label: "Fleet supply", used: 0, total: 3 }]);
  // Moving nothing is a movement: the main button says so.
  expect(labels(movement, local())).toEqual(["Move nothing"]);
});

it("shows the staged movement in the panel and on the board", () => {
  const state = local({ movement: fleet, inspected: "01" });
  const view = tactical(movement, state);
  const content = view.task.content;
  if (content.kind !== "movement") {
    throw new Error("not the movement step");
  }
  expect(view.tabs[1].caption).toBe("2 ships");
  expect(labels(movement, state)).toEqual(["Reset selection", "Move fleet"]);
  expect(content.origins[0].leaves).toBe("2 ships · 3 cargo");
  // Without the catalogue a unit is named by its base unit.
  expect(content.arrival).toEqual([
    { unit: "carrier", name: "Carrier", now: 0, after: 1 },
    { unit: "destroyer", name: "Destroyer", now: 0, after: 1 },
    { unit: "fighter", name: "Fighter", now: 0, after: 1 },
    { unit: "infantry", name: "Infantry", now: 0, after: 2 },
  ]);
  expect(content.gauges).toEqual([
    { label: "Fleet supply", used: 2, total: 3 },
    { label: "Transport capacity", used: 3, total: 6 },
  ]);
  const carrier = content.origins[0].ships[0].units[0];
  expect(carrier).toMatchObject({ moves: true, route: "Direct", riftRoll: false, boost: null });
  expect(carrier.hold).toMatchObject({ capacity: 6, loaded: 3 });
  expect(carrier.hold!.slots.map((slot) => slot.unit)).toEqual(["fighter", "infantry", "infantry"]);
  // The second carrier stays, and can still move.
  expect(content.origins[0].ships[0].units[1]).toMatchObject({ moves: false, canMove: true });

  const { board } = selectShell(movement, state);
  // The sheet of the system takes the place of its inspector.
  expect(board.origin?.system).toBe("01");
  expect(board.inspector).toBeNull();
  expect(board.routes).toEqual([{ id: "01>23", path: ["01", "23"], staged: true, marks: [] }]);
  const home = board.tiles.find((tile) => tile.id === "01")!;
  // Two carriers, a destroyer and three fighters; a carrier, the destroyer and a fighter leave.
  expect(home.fleets).toEqual([{ seat: "a", ships: 3, strength: 0, was: 6 }]);
  expect(home.planets[0].groundForces).toBe(3);
  expect(home.handled).toBe(true);
  // The origins are the choice on the board, so a phone stays on the map.
  expect(board.task).toMatchObject({
    interactive: true,
    verb: "Move from",
    chosen: { "01": true },
  });
  expect(Object.keys(board.task!.values)).toEqual(board.taskSystems.filter((id) => id !== "23"));
  expect(carrier.hold!.slots.map((slot) => slot.place)).toEqual([
    "Space area",
    "Planet Jord",
    "Planet Jord",
  ]);
  // A system where nothing is staged has the mark only when the player set it.
  const marked = (handled: string[]) =>
    selectShell(movement, local({ handled })).board.tiles.find((tile) => tile.id === "01")!.handled;
  expect([marked([]), marked(["01"])]).toEqual([false, true]);
  const target = board.tiles.find((tile) => tile.id === "23")!;
  expect(target.fleets).toEqual([{ seat: "a", ships: 3, strength: 0, was: 0 }]);
  expect(board.taskSystems.sort()).toEqual(["01", "23"]);
});

it("lists what is committed to the active system, read-only", () => {
  const { board } = selectShell(movement, local({ movement: fleet, inspected: "23" }));
  expect(board.origin).toBeNull();
  expect(board.inspector?.arriving?.summary).toBe("2 ships · 3 cargo · fleet supply 2 / 3");
  expect(
    board.inspector?.arriving?.origins[0].ships.map((ship) => [ship.name, ship.units.length]),
  ).toEqual([
    ["Carrier", 1],
    ["Destroyer", 1],
  ]);
  expect(board.inspector?.arriving?.origins[0].cargo).toEqual([]);
});

it("opens the activation of a tactical action that places no command token", () => {
  // Warfare of Thunder's Edge: any system, and no token of the tactic pool.
  const choice = activation.pending_choice!.choice;
  const free = {
    ...activation,
    pending_choice: {
      ...activation.pending_choice!,
      choice: { ...choice, context: { ...choice.context!, subtype: "warfare_free_tactical" } },
    },
    tactical: { ...activation.tactical!, costs_token: false },
  } as SessionUpdate;
  expect(tactical(free, local()).task.content).toMatchObject({
    kind: "activation",
    costsToken: false,
  });
  expect(tactical(activation, local()).task.content).toMatchObject({ costsToken: true });
});

it("opens no resolved step in Live: its tab is locked, and the staged movement decides", () => {
  // The result of the activation is in the live turn of the Log.
  const view = tactical(movement, local({ step: 0, movement: fleet }));
  expect(view.selected).toBe(1);
  expect(view.current).toBe(1);
  expect(view.tabs.map((tab) => !!tab.locked)).toEqual([true, false, false, false, false]);
  expect(view.task.role).toBe("decide");
  expect(view.task.content).toMatchObject({ kind: "movement" });
  expect(labels(movement, local({ step: 0, movement: fleet }))).toContain("Move fleet");
});

it("says in the footer why a plan was refused", () => {
  const refused = local({ planNote: { text: "The game does not offer that.", error: true } });
  expect(tactical(movement, refused).task.footer).toMatchObject({
    note: "The game does not offer that.",
    error: true,
  });
});

it("has no screen of its own once the movement is sent, or for a hold", () => {
  const sent = selectShell(movement, local({ sent: movement.pending_choice!.nonce }));
  expect(sent.action.kind).toBe("flow");
  expect(sent.board.origin).toBeNull();
  expect(selectShell(loadUpdate as unknown as SessionUpdate, local()).action.kind).toBe("flow");
});

it("says the move value of a kind in its heading, and of a ship where it differs", () => {
  const ships = (update: SessionUpdate, state: LocalState) => {
    const content = tactical(update, state).task.content;
    if (content.kind !== "movement") {
      throw new Error("not the movement step");
    }
    return content.origins[0].ships;
  };
  const [carriers, destroyers] = ships(movement, local({ movement: fleet }));
  expect(carriers.facts).toEqual(["Move 1 · Capacity 6"]);
  expect(destroyers.facts).toEqual(["Move 2"]);
  expect(carriers.units.map((unit) => unit.move)).toEqual([null, null]);

  // The first carrier needs Gravity Drive, and the destroyer starts in a nebula.
  const changed: SessionUpdate = {
    ...movement,
    tactical: {
      ...facts,
      ships: facts.ships.map((ship) =>
        ship.unit === "destroyer"
          ? { ...ship, nebula: true }
          : ship.index === 0
            ? { ...ship, move: { ...ship.move!, gravity_drive: true } }
            : ship,
      ),
    },
  };
  const [boosted, capped] = ships(changed, local({ movement: fleet }));
  expect(boosted.facts).toEqual(["Move 1 · Capacity 6"]);
  expect(boosted.units.map((unit) => unit.move)).toEqual(["Move 1 → 2", null]);
  expect(capped.facts).toEqual(["Move 2 → 1 · Nebula"]);
  // A ship that stays says nothing of a boost.
  expect(ships(changed, local())[0].units.map((unit) => unit.move)).toEqual([null, null]);
});

it("names every unit of the movement as its card prints it", () => {
  const content = selectMovement(movement, facts, fleet, [], null, [], printed);
  expect(content.arrival).toEqual([
    { unit: "carrier", name: "Advanced Carrier I", now: 0, after: 1 },
    { unit: "destroyer", name: "Destroyer I", now: 0, after: 1 },
    { unit: "fighter", name: "Fighter I", now: 0, after: 1 },
    { unit: "infantry", name: "Spec Ops I", now: 0, after: 2 },
  ]);
  const origin = content.origins[0];
  expect(origin.ships.map((ship) => ship.name)).toContain("Advanced Carrier I");
  expect(origin.cargo.map((cargo) => cargo.name)).toEqual(
    expect.arrayContaining(["Spec Ops I", "Fighter I"]),
  );
  const carrier = origin.ships.find((ship) => ship.name === "Advanced Carrier I")!;
  expect(carrier.units[0].hold!.slots.map((slot) => slot.name)).toEqual([
    "Fighter I",
    "Spec Ops I",
    "Spec Ops I",
  ]);
});
