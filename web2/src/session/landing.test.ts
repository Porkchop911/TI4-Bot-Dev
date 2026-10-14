import { expect, it } from "vitest";
import landingUpdate from "./fixtures/update-landing.json";
import {
  type LandingDraft,
  chosenForce,
  draftOf,
  forceLeft,
  landKey,
  landed,
  setCount,
  setLanding,
  stepsOf,
} from "./landingDraft";
import { type PlanStep, stepOf } from "./movementPlan";
import type { LocalState } from "./select/action";
import { selectShell } from "./select/shell";
import type { InvasionFacts, SessionUpdate } from "./wire";

// An update of a real game, written by the engine: seat a (Sol) moved a carrier with two
// infantry to the system next to its home, which has one planet that nobody holds.
//   cargo run -p ti4-wasm --example record -- 3 8 1 10 pok8imperial no tactical 23 'move|01|0' 'load|3' 'load|4' done_loading done_moving > web2/src/session/fixtures/update-landing.json
const update = landingUpdate as unknown as SessionUpdate;
const real = update.tactical as InvasionFacts;
const choice = update.pending_choice!.choice;

/** Two infantry and a mech in space, and a damaged mech on a planet next door; three planets. */
const facts: InvasionFacts = {
  kind: "invasion",
  active: "38",
  forces: [
    { unit: "sol_infantry", damaged: false, count: 2 },
    { unit: "sol_mech", damaged: false, count: 1 },
    { unit: "sol_mech", damaged: true, from_system: "63", from_planet: "vorhal", count: 1 },
  ],
  planets: [
    { planet: "abyz", lands: [0, 1, 2] },
    { planet: "fria", lands: [0, 1] },
    { planet: "station" },
  ],
};
const staged = (...lands: [planet: string, force: number, count: number][]) =>
  lands.reduce(
    (draft, [planet, force, count]) => setLanding(facts, draft, planet, force, count),
    {} as LandingDraft,
  );

it("has the facts of the engine for the fixture", () => {
  expect(real).toEqual({
    kind: "invasion",
    active: "23",
    forces: [{ unit: "sol_infantry", damaged: false, count: 2 }],
    planets: [{ planet: "saudor", lands: [0] }],
  });
});

it("lands within what is left of a force, and only where the force can land", () => {
  const draft = staged(["abyz", 0, 1], ["fria", 0, 5]);
  expect(draft).toEqual({ abyz: { 0: 1 }, fria: { 0: 1 } });
  expect(forceLeft(facts, draft, 0)).toBe(0);
  expect(landed(draft)).toBe(2);
  // Nothing lands on the third planet, and the force from next door lands on one planet only.
  expect(staged(["station", 0, 1])).toEqual({});
  expect(staged(["fria", 2, 1])).toEqual({});
  expect(staged(["abyz", 2, 1])).toEqual({ abyz: { 2: 1 } });
  // A count of 0 takes the units back, and an empty planet leaves the draft.
  expect(setLanding(facts, draft, "fria", 0, 0)).toEqual({ abyz: { 0: 1 } });
  expect(setCount(facts, draft, landKey("abyz", 0), 0)).toEqual({ fria: { 0: 1 } });
  expect(setCount(facts, draft, "ship:01|0", 1)).toBe(draft);
});

it("lands the chosen kind while units of it are left, then the first that is left", () => {
  expect(chosenForce(facts, {}, null)).toBe(0);
  expect(chosenForce(facts, {}, 1)).toBe(1);
  const draft = staged(["abyz", 1, 1]);
  expect(chosenForce(facts, draft, 1)).toBe(0);
  // On a planet, only what can land there.
  expect(chosenForce(facts, {}, 2, "fria")).toBe(0);
  expect(chosenForce(facts, staged(["abyz", 0, 2], ["abyz", 1, 1]), null, "fria")).toBeNull();
  expect(chosenForce(facts, {}, null, "station")).toBeNull();
});

it("turns the staged landings into steps, and steps into the staged landings", () => {
  const draft = staged(["abyz", 0, 2], ["fria", 1, 1], ["abyz", 2, 1]);
  const steps = stepsOf(facts, draft);
  const infantry = {
    kind: "land",
    planet: "abyz",
    unit: "sol_infantry",
    damaged: false,
    from_system: null,
    from_planet: null,
  };
  expect(steps).toEqual([
    infantry,
    infantry,
    {
      kind: "land",
      planet: "abyz",
      unit: "sol_mech",
      damaged: true,
      from_system: "63",
      from_planet: "vorhal",
    },
    { ...infantry, planet: "fria", unit: "sol_mech" },
    { kind: "done_committing" },
  ]);
  expect(draftOf(facts, steps)).toEqual(draft);
  // A step that no longer fits is left out.
  const fewer: InvasionFacts = { ...facts, forces: [{ ...facts.forces[0], count: 1 }] };
  expect(draftOf(fewer, steps)).toEqual({ abyz: { 0: 1 } });
  expect(stepsOf(facts, {})).toEqual([{ kind: "done_committing" }]);
});

it("reads the step back from the option that answered it", () => {
  const steps: PlanStep[] = stepsOf(real, setLanding(real, {}, "saudor", 0, 2));
  expect(steps).toHaveLength(3);
  const commit = choice.options.find((option) => option.kind === "commit")!;
  expect(stepOf(commit)).toEqual(steps[0]);
  expect(stepOf(choice.options.find((option) => option.id === "done_committing")!)).toEqual({
    kind: "done_committing",
  });
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

it("opens the invasion step: the planets are the choice on the board, the panel says what lands", () => {
  const view = selectShell(update, local({ inspected: "23" }));
  const { action, board } = view;
  if (action.kind !== "tactical" || action.task.content.kind !== "invasion") {
    throw new Error("the choice has no invasion step");
  }
  expect(action.tabs.map((tab) => tab.status)).toEqual([
    "done",
    "done",
    "done",
    "decision",
    "future",
  ]);
  expect(action.task.title).toBe("Invasion");
  expect(action.task.footer.actions.map((item) => item.label)).toEqual(["Land nothing"]);
  const landing = action.task.content.landing!;
  expect(landing.forces).toMatchObject([
    { id: "force:0", unit: "infantry", place: "Space area", site: null, left: 2 },
  ]);
  expect(landing.chosen).toBe("force:0");
  expect(landing.planets).toMatchObject([
    { id: "saudor", owner: null, there: [], tokens: [], lands: "", reason: null },
  ]);
  // Nobody controls the planet: who lands there explores it, into the deck of its type.
  expect(landing.planets[0].traits).toEqual({
    types: ["industrial"],
    unexplored: true,
    attachments: [],
  });
  expect(landing.stays).toBe("2 Infantry");
  expect(view.accent).toBe("live");
  // The board: the planet is the choice, and the sheet is open at the active system.
  expect(board.task).toMatchObject({
    target: "planet",
    kind: "pick",
    interactive: true,
    values: { saudor: 0 },
    chosen: {},
  });
  expect(board.landing).toEqual(landing);
  expect(board.inspector).toBeNull();
  expect(selectShell(update, local()).board.landing).toBeNull();
});

it("shows what is staged: in the panel, on the planet of the map, and in the footer", () => {
  const draft = setLanding(real, {}, "saudor", 0, 2);
  const view = selectShell(update, local({ landing: draft, canUndo: true }));
  const { action, board } = view;
  if (action.kind !== "tactical" || action.task.content.kind !== "invasion") {
    throw new Error("the choice has no invasion step");
  }
  expect(action.tabs[3].caption).toBe("2 units");
  expect(action.task.footer.actions.map((item) => item.label)).toEqual([
    "Undo",
    "Reset selection",
    "Commit ground forces",
  ]);
  const landing = action.task.content.landing!;
  expect(landing.planets[0].lands).toBe("2 Infantry");
  expect(landing.planets[0].tokens).toHaveLength(2);
  expect(landing.planets[0].accepts).toEqual({});
  expect(landing.stays).toBe("");
  expect(landing.chosen).toBeNull();
  expect(view.accent).toBe("draft");
  expect(board.task?.chosen).toEqual({ saudor: true });
  const before = selectShell(update, local()).board.tiles.find((tile) => tile.id === "23")!;
  const after = board.tiles.find((tile) => tile.id === "23")!;
  expect(after.planets[0].groundForces).toBe(before.planets[0].groundForces + 2);
});
