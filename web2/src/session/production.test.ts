import { expect, it } from "vitest";
import productionUpdate from "../../../fixtures/protocol/update-production.json";
import { type PlanStep, stepOf } from "./movementPlan";
import {
  EMPTY,
  type ProductionDraft,
  addBuild,
  autoPay,
  blocked,
  overrun,
  draftOf,
  removeBuild,
  setCount,
  setGoods,
  sizes,
  stagedCounts,
  stepsOf,
  togglePlanet,
  totals,
} from "./productionDraft";
import type { LocalState } from "./select/action";
import { selectShell } from "./select/shell";
import type { ProductionFacts, SessionUpdate, TacticalFacts } from "./wire";

// An update of a real game, written by the engine: in the scenario "Late game" seat a (Sol)
// activated the system with its two space docks, moved nothing and kept AI Development Algorithm
// (`crates/ti4-wasm/tests/it/protocol_fixtures.rs` (`cargo wire-web`)).
const update = productionUpdate as unknown as SessionUpdate;
const facts = update.tactical as Extract<TacticalFacts, { kind: "production" }>;
const choice = update.pending_choice!.choice;

const unit = (id: string) => facts.units.findIndex((item) => item.unit === id);
const [FIGHTER, INFANTRY, CRUISER, WARSUN, DREADNOUGHT, CARRIER] = [
  "fighter2",
  "sol_infantry2",
  "cruiser2",
  "warsun",
  "dreadnought2",
  "sol_carrier2",
].map(unit);
const built = (...builds: [unit: number, place: string][]) =>
  builds.reduce((draft, [kind, place]) => addBuild(facts, draft, kind, place), EMPTY);

it("has the facts of the engine for the fixture", () => {
  expect(facts).toMatchObject({
    kind: "production",
    system: "72",
    capacity: { limit: 13, used: 0 },
    discount: 1,
    credit: 0,
    trade_goods: 6,
    trade_good_worth: 1,
    fleet_supply: { limit: 5, charged: 3 },
  });
  // Every unit of a production, also the ones that the seat cannot produce now.
  expect(facts.units.map((item) => [item.unit, item.unavailable])).toEqual([
    ["sol_carrier2", "supply"],
    ["cruiser2", undefined],
    ["destroyer2", undefined],
    ["dreadnought2", undefined],
    ["sol_flagship", "supply"],
    ["warsun", undefined],
    ["fighter2", undefined],
    ["sol_infantry2", undefined],
    ["sol_mech", "supply"],
  ]);
  expect(facts.units[INFANTRY]).toEqual({
    unit: "sol_infantry2",
    cost: 1,
    per: 2,
    cargo: true,
    places: ["lisis", "velnor"],
    combat: { hits_on: 6, dice: 1 },
    move_value: 0,
    capacity: 0,
    rules:
      "After this unit is destroyed, roll 1 die. If the result is 5 or greater, place the unit on this card. At the start of your next turn, place each unit that is on this card on a planet you control in your home system.",
  });
  expect(facts.units[WARSUN]).toMatchObject({ cost: 12, per: 1, supply: 1, fleet: true });
  expect(facts.planets).toContainEqual({ planet: "lisis", worth: 2 });
  // The exhausted planet of the scenario does not pay.
  expect(facts.planets.map((planet) => planet.planet)).not.toContain("quann");
});

it("adds up the staged purchases: units, cost after the discount, and the counts of the fleet", () => {
  const draft = built([CRUISER, "space"], [FIGHTER, "space"], [INFANTRY, "velnor"]);
  expect(sizes(facts, draft)).toEqual([1, 2, 2]);
  expect(totals(facts, draft)).toMatchObject({
    units: 5,
    capacity: 13,
    cost: 3,
    paid: 0,
    available: 26,
  });
  // The counts that the engine reads the fleet supply and the capacity from: the cruiser is a
  // ship with a capacity of one, the fighters are fighters in space, the infantry is on a planet.
  expect(stagedCounts(facts, draft)).toEqual({
    ...facts.fleet,
    ships: facts.fleet.ships + 1,
    transport: facts.fleet.transport + 1,
    fighters: facts.fleet.fighters + 2,
  });
  // The space docks of the system hold fighters, and Sol has Fighter II: the engine says so.
  expect(facts.fleet.fighter_support).toBeGreaterThan(0);
  expect(facts.fleet.fighters_to_fleet_pool).toBe(true);
  // The capacity follows the staged ships, and goes back with them.
  const sun = built([WARSUN, "space"]);
  expect(stagedCounts(facts, sun).transport).toBe(facts.fleet.transport + 6);
  expect(stagedCounts(facts, removeBuild(sun, 0))).toEqual(facts.fleet);
  expect(totals(facts, EMPTY)).toMatchObject({ units: 0, cost: 0 });
  // A purchase only where the unit can be placed.
  expect(addBuild(facts, EMPTY, INFANTRY, "space")).toBe(EMPTY);
  expect(addBuild(facts, EMPTY, CRUISER, "lisis")).toBe(EMPTY);
  expect(removeBuild(draft, 1).builds).toEqual([
    { unit: CRUISER, place: "space" },
    { unit: INFANTRY, place: "velnor" },
  ]);
});

it("stops at the supply, and stages past the production limit, where a pair makes one unit", () => {
  const sun = built([WARSUN, "space"]);
  expect(blocked(facts, sun, WARSUN)).toBe("None left in supply");
  expect(addBuild(facts, sun, WARSUN, "space")).toBe(sun);
  // Six pairs of fighters are 12 units of 13: the next pair makes one, which is not over.
  let draft = EMPTY;
  for (let pair = 0; pair < 7; pair++) {
    draft = addBuild(facts, draft, FIGHTER, "space");
  }
  expect(sizes(facts, draft)).toEqual([2, 2, 2, 2, 2, 2, 1]);
  expect(totals(facts, draft)).toMatchObject({ units: 13, cost: 6 });
  expect(overrun(facts, draft).capacity).toBe(0);
  // The production limit is soft: one more cruiser is staged, and the build is one unit over.
  draft = addBuild(facts, draft, CRUISER, "space");
  expect(blocked(facts, draft, CRUISER)).toBeNull();
  expect(sizes(facts, draft).at(-1)).toBe(1);
  expect(overrun(facts, draft).capacity).toBe(1);
  // A unit that the game does not offer is never staged.
  expect(blocked(facts, EMPTY, CARRIER)).toBe("None left in supply");
  expect(addBuild(facts, EMPTY, CARRIER, "space")).toBe(EMPTY);
});

it("counts the purchases of a unit in a place, and the resources do not stop a build", () => {
  const key = (kind: number, place: string) => `build:${kind}:${place}`;
  let draft = setCount(facts, EMPTY, key(INFANTRY, "lisis"), 2);
  draft = setCount(facts, draft, key(CRUISER, "space"), 1);
  draft = setCount(facts, draft, key(INFANTRY, "velnor"), 1);
  expect(draft.builds).toEqual([
    { unit: INFANTRY, place: "lisis" },
    { unit: INFANTRY, place: "lisis" },
    { unit: CRUISER, place: "space" },
    { unit: INFANTRY, place: "velnor" },
  ]);
  // One less takes the last purchase of that unit and place back.
  expect(setCount(facts, draft, key(INFANTRY, "lisis"), 1).builds).toEqual([
    { unit: INFANTRY, place: "lisis" },
    { unit: CRUISER, place: "space" },
    { unit: INFANTRY, place: "velnor" },
  ]);
  expect(setCount(facts, draft, key(INFANTRY, "space"), 1)).toBe(draft);
  expect(setCount(facts, draft, "other", 1)).toBe(draft);
  // A war sun, a dreadnought and three cruisers cost more than the seat has: it is staged.
  const much = setCount(
    facts,
    built([WARSUN, "space"], [DREADNOUGHT, "space"]),
    key(CRUISER, "space"),
    3,
  );
  expect(much.builds).toHaveLength(5);
  expect(totals(facts, much)).toMatchObject({ cost: 21, available: 26 });
});

it("stages a payment: planets by a tap, trade goods by a counter, and the least waste", () => {
  const draft = built([CRUISER, "space"], [FIGHTER, "space"], [INFANTRY, "velnor"]);
  const lisis = togglePlanet(facts, draft, "lisis");
  expect(totals(facts, lisis).paid).toBe(2);
  expect(togglePlanet(facts, lisis, "lisis").planets).toEqual([]);
  // A planet that cannot pay is not staged.
  expect(togglePlanet(facts, draft, "quann")).toBe(draft);
  expect(setGoods(facts, draft, 99).goods).toBe(6);
  expect(totals(facts, setGoods(facts, lisis, 1))).toMatchObject({ cost: 3, paid: 3 });
  // 3 resources: one planet of 3, not two planets and not trade goods.
  const auto = autoPay(facts, setGoods(facts, lisis, 2));
  expect(auto.goods).toBe(0);
  expect(auto.planets).toHaveLength(1);
  expect(totals(facts, auto)).toMatchObject({ cost: 3, paid: 3 });
  // 11 for a war sun: the planets pay it exactly.
  const sun = autoPay(facts, built([WARSUN, "space"]));
  expect(totals(facts, sun)).toMatchObject({ cost: 11, paid: 11 });
  expect(sun.goods).toBe(0);
});

it("is sent as its purchases with their places, then the payment, and staged again from them", () => {
  const draft = setGoods(
    facts,
    togglePlanet(
      facts,
      built([CRUISER, "space"], [INFANTRY, "velnor"], [INFANTRY, "lisis"]),
      "jord",
    ),
    1,
  );
  const steps = stepsOf(facts, draft);
  expect(steps).toEqual([
    { kind: "produce", unit: "cruiser2" },
    { kind: "produce", unit: "sol_infantry2" },
    { kind: "place", destination: "velnor" },
    { kind: "produce", unit: "sol_infantry2" },
    { kind: "place", destination: "lisis" },
    { kind: "pay", source: "exhaust|jord" },
    { kind: "pay", source: "trade_good" },
    { kind: "done_producing" },
  ]);
  expect(draftOf(facts, steps)).toEqual(draft);
  // The order of the answers of the game, as an undo tells them.
  const asked: PlanStep[] = [steps[0], steps[5], steps[1], steps[2], steps[3], steps[6], steps[4]];
  expect(draftOf(facts, asked)).toEqual(draft);
  // A payment alone is of another decision.
  expect(draftOf(facts, [steps[5], steps[6]])).toEqual(EMPTY);
  expect(stepsOf(facts, EMPTY)).toEqual([{ kind: "done_producing" }]);
});

it("reads the step back from the option that answered it", () => {
  const cruiser = choice.options.find((option) => option.id === "build|cruiser2|1")!;
  expect(stepOf(cruiser)).toEqual({ kind: "produce", unit: "cruiser2" });
  // No two options are the same step.
  const steps = choice.options.map((option) => JSON.stringify(stepOf(option)));
  expect(steps).not.toContain("null");
  expect(new Set(steps).size).toBe(steps.length);
  expect(stepOf({ id: "exhaust|jord", kind: "pay", payload: {} })).toEqual({
    kind: "pay",
    source: "exhaust|jord",
  });
  expect(stepOf({ id: "place|velnor", kind: "place", payload: { destination: "velnor" } })).toEqual(
    {
      kind: "place",
      destination: "velnor",
    },
  );
});

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
  production: EMPTY,
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

const production = (view: ReturnType<typeof selectShell>) => {
  const { action } = view;
  if (action.kind !== "tactical" || action.task.content.kind !== "production") {
    throw new Error("the choice has no production step");
  }
  return { action, sheet: action.task.content.sheet!, footer: action.task.footer };
};
const labels = (footer: { actions: { label: string }[] }) =>
  footer.actions.map((item) => item.label);

it("opens the production step with the build at the system that produces", () => {
  const view = selectShell(update, local({ inspected: "72" }));
  const { action, sheet, footer } = production(view);
  expect(action.tabs.map((tab) => tab.status)).toEqual([
    "done",
    "done",
    "done",
    "done",
    "decision",
  ]);
  expect(action.task.title).toBe("Production");
  expect(labels(footer)).toEqual(["Produce nothing"]);
  expect(footer.payment).toBeUndefined();
  expect(sheet.groups.map((group) => [group.title, group.byPlace, group.units.length])).toEqual([
    ["Ships · Space area", false, 7],
    ["Ground forces", true, 2],
  ]);
  const [ships, forces] = sheet.groups.map((group) => group.units);
  expect(ships.map((item) => item.name)).toEqual([
    "Carrier",
    "Cruiser",
    "Destroyer",
    "Dreadnought",
    "Flagship",
    "War sun",
    "Fighter",
  ]);
  expect(ships[1]).toEqual({
    id: `build-unit:${CRUISER}`,
    unit: "cruiser",
    name: "Cruiser",
    price: "2",
    combat: "6+",
    move: "3",
    capacity: "1",
    left: 3,
    abilities: [],
    rules: "",
    unavailable: null,
    counts: [
      {
        place: "Space area",
        counter: { id: `build:${CRUISER}:space`, value: 0, max: 1, label: "Cruiser" },
        made: 0,
        reason: null,
      },
    ],
  });
  expect(ships[5]).toMatchObject({
    price: "12",
    combat: "3+ (x3)",
    capacity: "6",
    left: 1,
    abilities: ["SUSTAIN DAMAGE", "BOMBARDMENT 3+ (x3)"],
    rules: "Other player's units in this system lose PLANETARY SHIELD.",
  });
  expect(ships[6]).toMatchObject({ price: "1 for 2", capacity: "–", left: null });
  // A unit that is not produced here keeps its row, with the reason and no counter.
  expect(ships[0]).toMatchObject({
    name: "Carrier",
    left: 0,
    unavailable: { word: "None left", reason: "None left in supply" },
    counts: [],
  });
  // A ground force has a counter for each planet with a dock.
  expect(forces[0].counts.map((count) => [count.place, count.counter.label])).toEqual([
    ["Planet Lisis", "Infantry, Planet Lisis"],
    ["Planet Velnor", "Infantry, Planet Velnor"],
  ]);
  expect(forces[0].move).toBe("–");
  expect(sheet.places.map((place) => place.name)).toEqual([
    "Space area",
    "Planet Lisis",
    "Planet Velnor",
  ]);
  expect(sheet.gauges).toEqual([
    { label: "Production capacity", used: 0, total: 13 },
    { label: "Resources", used: 0, total: 26 },
    { label: "Fleet supply after", used: 3, total: 5 },
    { label: "Fighters and cargo in space", used: 0, total: 9 },
  ]);
  // The board: the system opens the build, and no planet pays yet.
  const { board } = view;
  expect(board.task).toMatchObject({ target: "system", interactive: true, values: { "72": 0 } });
  expect(board.task?.payment).toBeUndefined();
  // The sheet on the board has what is in the system, as the inspector has.
  const { details, ...build } = board.production!;
  expect(build).toEqual(sheet);
  expect(details?.rows.flatMap((row) => row.planet?.name ?? [])).toEqual(["Lisis", "Velnor"]);
  expect(board.inspector).toBeNull();
  expect(board.taskSystems).toEqual(["72"]);
  expect(selectShell(update, local()).board.production).toBeNull();
  expect(view.accent).toBe("live");
});

it("shows the staged build: the counters, the cost, the limits, and the board after it", () => {
  const draft = built([CRUISER, "space"], [FIGHTER, "space"], [INFANTRY, "velnor"]);
  // The engine read the counts of the build: the space docks hold the two fighters.
  const standing = {
    fleet_limit: 5,
    fleet_charged: 4,
    transport: 10,
    consumed: 0,
    fighters_charged: 0,
    capacity_excess: 0,
  };
  const view = selectShell(
    update,
    local({
      inspected: "72",
      production: draft,
      canUndo: true,
      standing: { key: JSON.stringify(stagedCounts(facts, draft)), response: standing },
    }),
  );
  const { action, sheet, footer } = production(view);
  expect(action.tabs[4].caption).toBe("5 units");
  expect(labels(footer)).toEqual(["Undo", "Reset selection", "Pay 3 resources"]);
  expect(sheet).toMatchObject({ made: 5, cost: 3, discount: 1 });
  expect(sheet.places.map((place) => place.builds)).toEqual([
    "1 Cruiser, 2 Fighters",
    "",
    "2 Infantry",
  ]);
  const [ships, ground] = sheet.groups.map((group) => group.units);
  // A counter counts purchases and shows the units: one purchase of fighters is two.
  expect(ships[6].counts[0]).toMatchObject({ counter: { value: 1, max: 2 }, made: 2 });
  expect(ground[0].counts.map((count) => count.made)).toEqual([0, 2]);
  expect(ships[1]).toMatchObject({ left: facts.units[CRUISER].supply! - 1 });
  expect(sheet.gauges).toEqual([
    { label: "Production capacity", used: 5, total: 13 },
    { label: "Resources", used: 3, total: 26 },
    { label: "Fleet supply after", used: 4, total: 5 },
    { label: "Fighters and cargo in space", used: 0, total: 10 },
  ]);
  expect(view.accent).toBe("draft");
  const tile = (shell: typeof view) => shell.board.tiles.find((item) => item.id === "72")!;
  const before = tile(selectShell(update, local()));
  const after = tile(view);
  expect(after.fleets[0].ships).toBe(before.fleets[0].ships + 3);
  const forces = (item: typeof after) =>
    Object.fromEntries(item.planets.map((planet) => [planet.id, planet.groundForces]));
  expect(forces(after)).toEqual({ ...forces(before), velnor: forces(before).velnor + 2 });
});

it("then asks for the payment on the map: the planets that pay, the bar, and the footer line", () => {
  const draft = built([CRUISER, "space"], [FIGHTER, "space"], [INFANTRY, "velnor"]);
  const stage = (change: Partial<ProductionDraft>) =>
    selectShell(
      update,
      local({ production: { ...draft, ...change }, productionStage: "pay" as const }),
    );
  const short = stage({ planets: ["lisis"] });
  expect(short.board.production).toBeNull();
  expect(short.board.task).toMatchObject({
    target: "planet",
    kind: "pay",
    interactive: true,
    chosen: { lisis: true },
    unit: "resources",
  });
  expect(short.board.task?.values).toMatchObject({ jord: 4, lisis: 2, velnor: 3 });
  expect(short.board.task?.values.quann).toBeUndefined();
  expect(short.board.task?.payment).toMatchObject({
    cost: 3,
    paid: 2,
    summary: "Lisis 2",
    goods: { id: "trade_good", value: 0, max: 6, rest: 1 },
  });
  expect(short.board.taskSystems).toContain("01");
  expect(short.board.fitKey).toBe("payment:72");
  const footer = production(short).footer;
  expect(labels(footer)).toEqual(["Reset selection", "Back to build", "Produce units"]);
  expect(footer.actions.at(-1)?.disabled).toBe(true);
  expect(footer.payment).toMatchObject({ cost: 3, paid: 2 });
  // Paid exactly, and paid with waste: both can be sent.
  const exact = stage({ planets: ["lisis"], goods: 1 });
  expect(production(exact).footer.actions.at(-1)?.disabled).toBe(false);
  expect(exact.board.task?.payment).toMatchObject({ paid: 3, summary: "Lisis 2 + 1 trade good" });
  expect(exact.board.task?.payment?.goods?.rest).toBeNull();
  const wasted = stage({ planets: ["jord"] });
  expect(wasted.board.task?.payment).toMatchObject({ cost: 3, paid: 4 });
  expect(production(wasted).footer.actions.at(-1)?.disabled).toBe(false);
});

it("does not go on with a build that the seat cannot pay", () => {
  // A war sun, a dreadnought and two cruisers: 20 less the discount is 19, more than the planets.
  const poor: SessionUpdate = {
    ...update,
    tactical: { ...facts, trade_goods: 0, planets: facts.planets.slice(0, -3) },
  };
  const having = (poor.tactical as ProductionFacts).planets.reduce((sum, p) => sum + p.worth, 0);
  const draft = built(
    [WARSUN, "space"],
    [DREADNOUGHT, "space"],
    [CRUISER, "space"],
    [CRUISER, "space"],
  );
  const { sheet, footer } = production(
    selectShell(poor, local({ inspected: "72", production: draft })),
  );
  expect(sheet.cost).toBe(19);
  expect(sheet.short).toBe(19 - having);
  expect(sheet.gauges[1]).toEqual({ label: "Resources", used: 19, total: having });
  expect(footer.actions.at(-1)).toMatchObject({ label: "Pay 19 resources", disabled: true });
  expect(footer.note).toBe(`${19 - having} resources short`);
  // The build is not blocked: a cheaper one can be reached in any order.
  expect(sheet.groups[0].units[2].counts[0].reason).toBeNull();
});

it("produces at once what costs nothing", () => {
  // Two fighters cost 1, and Sarween Tools takes 1 off.
  const view = selectShell(update, local({ production: built([FIGHTER, "space"]) }));
  expect(labels(production(view).footer)).toEqual(["Reset selection", "Produce units"]);
  expect(production(view).sheet.cost).toBe(0);
});

it("pays with the face that the engine names, and with commodities as trade goods", () => {
  // Archon's Gift: Lisis pays with its influence. The Keleres agent: two commodities pay.
  const gifted: ProductionFacts = {
    ...facts,
    planets: facts.planets.map((planet) =>
      planet.planet === "lisis" ? { ...planet, worth: 4, face: "influence" } : planet,
    ),
    commodities: 2,
  };
  let draft = built([CRUISER, "space"], [FIGHTER, "space"], [INFANTRY, "velnor"]);
  draft = togglePlanet(gifted, draft, "lisis");
  draft = setGoods(gifted, draft, 1, "commodity");
  expect(totals(gifted, draft)).toMatchObject({ cost: 3, paid: 5, available: 26 + 2 + 2 });
  const steps = stepsOf(gifted, draft);
  expect(steps.filter((step) => step.kind === "pay")).toEqual([
    { kind: "pay", source: "exhaust|lisis|influence" },
    { kind: "pay", source: "commodity" },
  ]);
  // Staged again from its steps, the payment is the same.
  const again = draftOf(gifted, steps as PlanStep[]);
  expect(again).toMatchObject({ planets: ["lisis"], goods: 0, commodities: 1 });
  // A commodity is spent before a trade good, and no more than the agent allows.
  expect(setGoods(gifted, draft, 5, "commodity").commodities).toBe(2);
  const paid = autoPay(gifted, { ...draft, planets: [], commodities: 0 });
  expect(totals(gifted, paid).paid).toBe(3);
});
