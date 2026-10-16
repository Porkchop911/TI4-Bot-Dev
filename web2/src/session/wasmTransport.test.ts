import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCatalog } from "@ti4/shared/content/catalog.ts";
import { expect, it } from "vitest";
import { setLoad, setShip } from "./movementDraft";
import {
  type Draft,
  activate,
  answer,
  entryOf,
  fits,
  frontier,
  newDraft,
  problemOf,
  scriptOf,
  setMovement,
  setProduction,
  stageAfter,
} from "./draft";
import { draftOf as landingOf, stepsOf as landingSteps, setLanding } from "./landingDraft";
import { type MovementStep, type PlanStep, draftOf, stepsOf } from "./movementPlan";
import {
  EMPTY,
  addBuild,
  draftOf as productionOf,
  stepsOf as productionSteps,
  setGoods,
  togglePlanet,
  totals,
} from "./productionDraft";
import { selectActionCards } from "./select/cards";
import { factionName } from "./select/names";
import { selectReaction } from "./select/reaction";
import { selectScore } from "./select/score";
import { selectAgendas } from "./select/agendas";
import { selectSecrets } from "./select/secrets";
import { factionSheet } from "./select/factionSheet";
import { selectPlayers } from "./select/players";
import type { ModelFile } from "@ti4/shared/engine/protocol.ts";
import type { TransportEvent } from "./transport";
import type { LocalGame } from "./savedGame";
import {
  type Engine,
  type HostSave,
  createWasmTransport,
  engineQuery,
  scenariosOf,
  setupDrawOf,
  setupOptionsOf,
  setupPreviewOf,
} from "./wasmTransport";
import type {
  DraftOutcome,
  DraftStep,
  InvasionFacts,
  MovementFacts,
  PlanScope,
  ProductionFacts,
  ReactionModes,
  SeatSwitch,
  SessionUpdate,
  TableRequest,
} from "./wire";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/** A game of seed 3 with seat a on this side, and the updates of it, one after the other. */
async function play(
  stepping = false,
  game: LocalGame = { seed: 3, players: 8, humans: 1 },
  saved: {
    /** The game of an earlier visit (`save()` of its `play`). */
    save?: HostSave;
    /** The cards that are never offered, by name: a short form of `modes`. */
    never?: readonly string[];
    modes?: ReactionModes;
    ruleSeats?: number;
    table?: TableRequest;
    waitMs?: number;
    models?: ModelFile[];
  } = {},
) {
  const engine: Engine = {
    module: await WebAssembly.compile(readFileSync(WASM)),
    hash: "test",
    scenarios: [],
    catalog: null,
  };
  /** What plays the game again, as the transport told it last: every decision of every seat, and the answers of this side among them. */
  let save: HostSave | undefined = saved.save;
  const { table, never: _, ...rest } = saved;
  const transport = createWasmTransport(engine, table ? { ...game, table } : game, {
    ...rest,
    modes: {
      ...saved.modes,
      ...Object.fromEntries((saved.never ?? []).map((name) => [name, "never"])),
    },
    onSave: (made) => {
      save = made;
    },
    stepping,
  });
  const seen: Update[] = [];
  /** The cards of each window that the engine passed, and the Never cards as they were told last. */
  const passed: string[][] = [];
  let never: string[] = [];
  let modes: ReactionModes = {};
  /** Who waits for the outcome of a draft script. */
  const drafts = new Map<DraftStep[], (outcome: DraftOutcome | null) => void>();
  let wake: (() => void) | null = null;
  transport.subscribe((event) => {
    if (event.kind === "error") {
      throw new Error(event.message);
    }
    if (event.kind === "update") {
      seen.push(event);
      wake?.();
    } else if (event.kind === "draft") {
      drafts.get(event.script)?.(event.outcome);
      drafts.delete(event.script);
    } else if (event.kind === "passed") {
      passed.push(event.cards);
    } else if (event.kind === "modes") {
      never = Object.keys(event.modes).filter((key) => event.modes[key] === "never");
      modes = event.modes;
    }
  });
  /** Asks for a draft and waits for its answer: the script comes back with it. */
  const draft = (script: DraftStep[], scope?: PlanScope) =>
    new Promise<DraftOutcome | null>((answer) => {
      drafts.set(script, answer);
      transport.draft(script, scope);
    });
  let read = 0;
  const next = async (): Promise<Update> => {
    while (seen.length <= read) {
      await new Promise<void>((resume) => (wake = resume));
    }
    return seen[read++];
  };
  const nonce = (event: Update) => event.update.pending_choice!.nonce;
  return {
    transport,
    next,
    nonce,
    draft,
    passed,
    never: () => never,
    modes: () => modes,
    save: () => save!,
    decisions: (): readonly string[] => save?.decisions ?? [],
    answers: (): readonly string[] => save?.answers ?? [],
    switches: (): readonly SeatSwitch[] => save?.switches ?? [],
  };
}

const shipsIn = (update: SessionUpdate, system: string) =>
  (update.view.board.systems[system]?.units ?? [])
    .filter((unit) => unit.owner === "a")
    .map((unit) => unit.unit_type)
    .sort();
const subtype = (event: Update) => event.update.pending_choice?.choice.context?.subtype;

it.skipIf(!built)(
  "sends a movement as one plan, takes a refused plan back, and undoes a movement whole",
  async () => {
    const { transport, next, nonce, answers } = await play();
    // Seat a takes a tactical action and activates the system next to its home.
    for (const answer of ["pok8imperial", "tactical", "23"]) {
      transport.submitChoice(nonce(await next()), answer);
    }
    const open = await next();
    expect(subtype(open)).toBe("movement_step");
    const facts = open.update.tactical as MovementFacts;
    expect(facts.moved).toBe(0);
    expect(shipsIn(open.update, "23")).toEqual([]);
    const home = shipsIn(open.update, "01");

    // A plan with a step that the game does not offer, after a ship that it does offer.
    let draft = setShip(facts, {}, "01|0", true);
    draft = setLoad(
      facts,
      draft,
      "01|0",
      facts.cargo.findIndex((pool) => pool.unit === "sol_infantry"),
      2,
    );
    draft = setLoad(
      facts,
      draft,
      "01|0",
      facts.cargo.findIndex((pool) => pool.unit === "fighter"),
      1,
    );
    const good = stepsOf(facts, setShip(facts, draft, "01|2", true));
    const dreadnought: MovementStep = {
      kind: "move",
      origin: "01",
      unit: "dreadnought",
      damaged: false,
      gravity_drive: false,
      ionian: false,
    };
    const bad = [...good.slice(0, -1), dreadnought, good.at(-1)!];
    transport.submitPlan(nonce(open), { destination: "23", steps: bad });
    const refused = await next();
    // Nothing of the plan is in the game: the same choice is open, on the same board.
    expect(refused.plan).toEqual({
      destination: "23",
      applied: 0,
      remaining: bad,
      interrupted: false,
      reason: "The game does not offer to move dreadnought from 01.",
    });
    expect(nonce(refused)).toBe(nonce(open));
    expect(answers()).toHaveLength(3);
    expect(shipsIn(refused.update, "23")).toEqual([]);
    expect(shipsIn(refused.update, "01")).toEqual(home);
    expect((refused.update.tactical as MovementFacts).moved).toBe(0);

    // The plan that fits: a carrier with two infantry and a fighter, and the destroyer.
    transport.submitPlan(nonce(refused), { destination: "23", steps: good });
    const after = await next();
    expect(after.plan).toBeUndefined();
    expect(subtype(after)).not.toBe("movement_step");
    expect(subtype(after)).not.toBe("load_cargo");
    expect(shipsIn(after.update, "23")).toEqual([
      "destroyer",
      "fighter",
      "sol_carrier",
      "sol_infantry",
      "sol_infantry",
    ]);
    // move, three loads, the closed hold, move, done: each is an answer of the saved game.
    expect(answers().slice(3)).toEqual([
      "move|01|0",
      "load|0",
      "load|3",
      "load|4",
      "done_loading",
      "move|01|1",
      "done_moving",
    ]);

    // One undo takes the movement back whole.
    expect(after.canUndo).toBe(true);
    transport.undo();
    const again = await next();
    expect(subtype(again)).toBe("movement_step");
    expect(nonce(again)).toBe(nonce(open));
    expect(answers()).toHaveLength(3);
    expect(shipsIn(again.update, "23")).toEqual([]);
    expect(shipsIn(again.update, "01")).toEqual(home);
    // The update tells what was taken back: the draft of the movement is staged from it.
    expect(again.undone).toEqual(good);
    expect(draftOf(facts, again.undone!)).toEqual(setShip(facts, draft, "01|2", true));

    // One more undo is one answer: the activation is open again.
    transport.undo();
    const activation = await next();
    expect(subtype(activation)).toBe("activate_system");
    expect(activation.undone).toBeUndefined();
    expect(answers()).toHaveLength(2);
    transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "sends the landings as one plan, takes a refused plan back, and undoes the landings whole",
  async () => {
    const { transport, next, nonce, answers } = await play();
    for (const answer of ["pok8imperial", "tactical", "23"]) {
      transport.submitChoice(nonce(await next()), answer);
    }
    // A carrier moves with two infantry to the system next to the home, which has one planet.
    const moving = await next();
    const ships = moving.update.tactical as MovementFacts;
    const infantry = ships.cargo.findIndex((pool) => pool.unit === "sol_infantry");
    transport.submitPlan(nonce(moving), {
      destination: "23",
      steps: stepsOf(ships, setLoad(ships, setShip(ships, {}, "01|0", true), "01|0", infantry, 2)),
    });
    const open = await next();
    expect(subtype(open)).toBe("commit_ground_forces");
    const facts = open.update.tactical as InvasionFacts;
    expect(facts.forces).toEqual([{ unit: "sol_infantry", damaged: false, count: 2 }]);
    const before = answers().length;
    const onPlanet = (update: SessionUpdate) =>
      (update.view.board.systems["23"]?.units ?? []).filter((unit) => unit.planet === "saudor");

    // A plan with a unit that the game does not offer, after one that it does offer.
    const good = landingSteps(facts, setLanding(facts, {}, "saudor", 0, 2));
    const mech: PlanStep = { ...(good[0] as PlanStep & { kind: "land" }), unit: "sol_mech" };
    const bad = [good[0], mech, good.at(-1)!];
    transport.submitPlan(nonce(open), { destination: "23", steps: bad });
    const refused = await next();
    expect(refused.plan).toEqual({
      destination: "23",
      applied: 0,
      remaining: bad,
      interrupted: false,
      reason: "The game does not offer to land sol_mech on saudor.",
    });
    expect(nonce(refused)).toBe(nonce(open));
    expect(answers()).toHaveLength(before);
    expect(onPlanet(refused.update)).toEqual([]);
    expect(refused.update.tactical).toEqual(facts);

    // The plan that fits. After the last unit the game does not ask for the end of the landings.
    transport.submitPlan(nonce(refused), { destination: "23", steps: good });
    const after = await next();
    expect(after.plan).toBeUndefined();
    expect(subtype(after)).not.toBe("commit_ground_forces");
    expect(answers().slice(before)).toEqual(["commit|0|saudor", "commit|0|saudor"]);
    expect(onPlanet(after.update).map((unit) => unit.unit_type)).toEqual([
      "sol_infantry",
      "sol_infantry",
    ]);

    // One undo takes the landings back whole, and tells them: they are staged again.
    transport.undo();
    const again = await next();
    expect(subtype(again)).toBe("commit_ground_forces");
    expect(nonce(again)).toBe(nonce(open));
    expect(answers()).toHaveLength(before);
    expect(again.undone).toEqual(good.slice(0, 2));
    expect(landingOf(again.update.tactical as InvasionFacts, again.undone!)).toEqual({
      saudor: { 0: 2 },
    });

    // One unit lands and one stays: the game is told the end of the landings.
    const one = landingSteps(facts, setLanding(facts, {}, "saudor", 0, 1));
    transport.submitPlan(nonce(again), { destination: "23", steps: one });
    const done = await next();
    expect(done.plan).toBeUndefined();
    expect(answers().slice(before)).toEqual(["commit|0|saudor", "done_committing"]);

    // One more undo after the landings takes the movement back whole.
    transport.undo();
    expect(subtype(await next())).toBe("commit_ground_forces");
    transport.undo();
    const movement = await next();
    expect(subtype(movement)).toBe("movement_step");
    expect(nonce(movement)).toBe(nonce(moving));
    transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "plays a saved game again, and still undoes its movement whole",
  async () => {
    const first = await play();
    for (const answer of ["pok8imperial", "tactical", "23"]) {
      first.transport.submitChoice(first.nonce(await first.next()), answer);
    }
    const open = await first.next();
    const facts = open.update.tactical as MovementFacts;
    const steps = stepsOf(facts, setShip(facts, setShip(facts, {}, "01|0", true), "01|2", true));
    first.transport.submitPlan(first.nonce(open), { destination: "23", steps });
    await first.next();
    const saved = first.save();
    // The hold of the carrier was declined, though the plan loads nothing.
    expect(first.answers().slice(3)).toEqual([
      "move|01|0",
      "done_loading",
      "move|01|1",
      "done_moving",
    ]);
    first.transport.close();

    // A new visit: the page does not know which answers were one plan.
    const engine: Engine = {
      module: await WebAssembly.compile(readFileSync(WASM)),
      hash: "test",
      scenarios: [],
      catalog: null,
    };
    let answers: readonly string[] = [];
    const transport = createWasmTransport(
      engine,
      { seed: 3, players: 8, humans: 1 },
      { save: saved, onSave: (made) => (answers = made.answers) },
    );
    const updates: Update[] = [];
    let wake: (() => void) | null = null;
    transport.subscribe((event) => {
      if (event.kind === "update") {
        updates.push(event);
        wake?.();
      }
    });
    const wait = async (count: number) => {
      while (updates.length < count) {
        await new Promise<void>((resume) => (wake = resume));
      }
      return updates[count - 1];
    };
    const resumed = await wait(1);
    expect(shipsIn(resumed.update, "23")).toEqual(["destroyer", "sol_carrier"]);
    transport.undo();
    const back = await wait(2);
    expect(subtype(back)).toBe("movement_step");
    expect((back.update.tactical as MovementFacts).moved).toBe(0);
    // Also in a new visit: the game that was played again says what its answers chose.
    expect(back.undone).toEqual(steps);
    expect(answers).toHaveLength(3);
    transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "runs a draft on a copy of the game, and applies it as one request",
  async () => {
    const { transport, next, nonce, answers, draft: ask } = await play();
    transport.submitChoice(nonce(await next()), "pok8imperial");
    const menu = await next();
    expect(subtype(menu)).toBe("action_menu");

    // The draft starts at the choice of a system: the tactical action is its first step.
    let draft = newDraft();
    const start = (await ask(scriptOf(entryOf(draft))))!;
    expect(start.stop).toEqual({ kind: "open" });
    expect(start.update.pending_choice?.nonce).toBe("draft");
    expect(start.update.tactical?.kind).toBe("activation");

    draft = activate(draft, "23");
    const activated = (await ask(scriptOf(entryOf(draft))))!;
    expect(fits(entryOf(draft), activated)).toBe(true);
    const facts = activated.movement?.tactical as MovementFacts;
    expect(facts).toMatchObject({ kind: "movement", active: "23", moved: 0 });
    // The token of the draft is spent in the draft only.
    const tokens = (update: SessionUpdate) =>
      update.view.players.find((player) => player.id === "a")!.tactic_tokens;
    expect(tokens(activated.update)).toBe(tokens(menu.update) - 1);

    const steps = stepsOf(facts, setShip(facts, setShip(facts, {}, "01|0", true), "01|2", true));
    draft = setMovement(draft, steps);
    const moved = (await ask(scriptOf(entryOf(draft))))!;
    expect(moved.stop).toEqual({ kind: "complete" });
    expect(fits(entryOf(draft), moved)).toBe(true);
    expect(shipsIn(moved.update, "23")).toEqual(["destroyer", "sol_carrier"]);
    // What the movement is staged against is still the fleet before it.
    expect(moved.movement?.tactical).toEqual(facts);

    // A step that the game does not offer is named, with how far the script fits.
    const bad = (await ask([
      ...scriptOf(entryOf(draft)).slice(0, 2),
      {
        kind: "move",
        origin: "01",
        unit: "dreadnought",
        damaged: false,
        gravity_drive: false,
        ionian: false,
      },
      { kind: "done_moving" },
    ]))!;
    // The refusal names the entry and what it names, not only its place.
    expect(bad.stop).toMatchObject({
      kind: "refused",
      step: 2,
      code: "not_offered",
      entry: { kind: "move", origin: "01", unit: "dreadnought" },
    });
    expect(bad.consumed).toBe(2);

    // Nothing of this is in the game.
    expect(answers()).toHaveLength(1);

    transport.applyDraft(nonce(menu), {
      choices: ["tactical", "23"],
      plan: { destination: "23", steps },
    });
    const after = await next();
    expect(after.plan).toBeUndefined();
    expect(shipsIn(after.update, "23")).toEqual(["destroyer", "sol_carrier"]);
    expect(answers().slice(1)).toEqual([
      "tactical",
      "23",
      "move|01|0",
      "done_loading",
      "move|01|1",
      "done_moving",
    ]);
    transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "takes a draft back whole when the game does not offer one of its choices",
  async () => {
    const { transport, next, nonce, answers } = await play();
    transport.submitChoice(nonce(await next()), "pok8imperial");
    const menu = await next();
    transport.applyDraft(nonce(menu), { choices: ["tactical", "no such system"], plan: null });
    const refused = await next();
    expect(refused.plan).toMatchObject({
      applied: 0,
      interrupted: false,
      reason: 'The game does not offer "no such system" here.',
    });
    expect(nonce(refused)).toBe(nonce(menu));
    expect(answers()).toHaveLength(1);
    transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "waits for a step before each decision of another seat, and plays the same game",
  async () => {
    const run = await play();
    const stepped = await play(true);
    const answersOf = ["pok8imperial", "tactical", "23"];
    for (const answer of answersOf) {
      run.transport.submitChoice(run.nonce(await run.next()), answer);
    }
    const reached = await run.next();

    let pauses = 0;
    let given = 0;
    let event = await stepped.next();
    while (given < answersOf.length || event.stepping) {
      if (event.stepping) {
        // No choice of this side is open; the update names the seat that decides.
        pauses += 1;
        expect(event.update.pending_choice).toBeUndefined();
        expect(event.update.viewer).toEqual({ role: "player", seat: "a" });
        expect(event.stepping.seat).not.toBe("a");
        if (pauses === 3) {
          // A draft can be run while another seat decides, also while it chooses its card.
          expect(event.update.view.phase).toBe("strategy");
          const drafted = await stepped.draft([{ kind: "choose", option_id: "tactical" }]);
          expect(drafted?.stop).toEqual({ kind: "open" });
          expect(drafted?.update.tactical?.kind).toBe("activation");
          // From here on the seats play on.
          stepped.transport.capabilities.stepping?.set(false);
        } else {
          stepped.transport.capabilities.stepping?.step();
        }
      } else {
        stepped.transport.submitChoice(stepped.nonce(event), answersOf[given]);
        given += 1;
      }
      event = await stepped.next();
    }
    expect(pauses).toBe(3);
    expect(event.update).toEqual(reached.update);
    run.transport.close();
    stepped.transport.close();
  },
  60_000,
);

it.skipIf(!built)("starts a scenario at the turn menu of seat a, on its board", async () => {
  const scenarios = await scenariosOf(await WebAssembly.compile(readFileSync(WASM)));
  const late = scenarios.find((scenario) => scenario.id === "late-game")!;
  expect(late).toMatchObject({ seed: 42, players: 8 });
  const { transport, next, nonce } = await play(false, {
    seed: late.seed,
    players: late.players,
    humans: 1,
    scenario: late.id,
  });
  const menu = await next();
  expect(nonce(menu)).toBe("1");
  expect(menu.update.view.round).toBe(4);
  // The fleet in the gravity rift, with its upgrades and the ground forces in its space area.
  expect(shipsIn(menu.update, "41")).toEqual([
    "cruiser2",
    "fighter2",
    "fighter2",
    "sol_carrier2",
    "sol_infantry2",
    "sol_infantry2",
  ]);
  transport.submitChoice(nonce(menu), "tactical");
  const activation = await next();
  const facts = activation.update.tactical;
  expect(facts?.kind).toBe("activation");
  transport.close();
});

it.skipIf(!built)(
  "sends a production as one plan, takes a refused plan back, and undoes a production whole",
  async () => {
    const { transport, next, nonce } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    // Seat a activates the system with its two space docks, moves nothing, and keeps AI
    // Development Algorithm.
    for (const answer of ["tactical", "72", "done_moving", "decline"]) {
      transport.submitChoice(nonce(await next()), answer);
    }
    const open = await next();
    expect(subtype(open)).toBe("produce_unit");
    expect(open.update.tactical?.kind).toBe("production");
    const facts = open.update.tactical as ProductionFacts;
    const unit = (id: string) => facts.units.findIndex((item) => item.unit === id);
    // The printed side of a unit, in the shape that the card reads. A failure here with an
    // engine that is older than `ti4-host/src/view`: run scripts/build-wasm.sh.
    expect(facts.units[unit("warsun")]).toMatchObject({
      abilities: ["SUSTAIN DAMAGE", "BOMBARDMENT 3+ (x3)"],
      rules: "Other player's units in this system lose PLANETARY SHIELD.",
    });
    const seat = (update: SessionUpdate) => update.view.players.find((item) => item.id === "a")!;
    const before = shipsIn(open.update, "72");

    // A cruiser, two fighters and two infantry on Velnor: 4 resources, 3 with Sarween Tools.
    let draft = addBuild(facts, EMPTY, unit("cruiser2"), "space");
    draft = addBuild(facts, draft, unit("fighter2"), "space");
    draft = addBuild(facts, draft, unit("sol_infantry2"), "velnor");
    expect(totals(facts, draft)).toMatchObject({ units: 5, cost: 3, paid: 0 });

    // With nothing to pay it, the game refuses the plan, and nothing of it is in the game.
    transport.submitPlan(nonce(open), { destination: "72", steps: productionSteps(facts, draft) });
    const refused = await next();
    expect(refused.plan).toMatchObject({ applied: 0, interrupted: false });
    expect(refused.plan?.reason).toMatch(/payment/);
    expect(subtype(refused)).toBe("produce_unit");
    expect(shipsIn(refused.update, "72")).toEqual(before);
    expect(productionOf(facts, refused.plan!.remaining)).toEqual(draft);

    // Lisis pays 2 and a trade good the rest.
    draft = setGoods(facts, togglePlanet(facts, draft, "lisis"), 1);
    expect(totals(facts, draft)).toMatchObject({ cost: 3, paid: 3 });
    const steps = productionSteps(facts, draft);
    transport.submitPlan(nonce(refused), { destination: "72", steps });
    const done = await next();
    expect(done.plan).toBeUndefined();
    expect(subtype(done)).not.toBe("produce_unit");
    expect(shipsIn(done.update, "72")).toEqual(
      [...before, "cruiser2", "fighter2", "fighter2", "sol_infantry2", "sol_infantry2"].sort(),
    );
    const velnor = done.update.view.board.systems["72"].units.filter(
      (item) => item.planet === "velnor" && item.unit_type === "sol_infantry2",
    );
    expect(velnor).toHaveLength(3);
    expect(done.update.view.board.systems["72"].planets.lisis.exhausted).toBe(true);
    expect(done.update.view.board.systems["72"].planets.velnor.exhausted).toBe(false);
    expect(seat(done.update).trade_goods).toBe(seat(open.update).trade_goods - 1);

    // One undo takes the whole production back, and tells its steps: it is staged again.
    transport.undo();
    const undone = await next();
    expect(subtype(undone)).toBe("produce_unit");
    expect(shipsIn(undone.update, "72")).toEqual(before);
    expect(undone.update.view.board.systems["72"].planets.lisis.exhausted).toBe(false);
    expect(productionOf(undone.update.tactical as ProductionFacts, undone.undone ?? [])).toEqual(
      draft,
    );
    transport.close();
  },
);

it.skipIf(!built)(
  "drafts a tactical action up to its production, and applies it as one request",
  async () => {
    const {
      transport,
      next,
      nonce,
      answers,
      draft: ask,
    } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    const menu = await next();
    expect(subtype(menu)).toBe("action_menu");
    const run = async (draft: Draft) => (await ask(scriptOf(entryOf(draft))))!;
    const asks = (outcome: DraftOutcome) => outcome.update.pending_choice?.choice.context?.subtype;

    // The system of the two space docks. Nothing moves: the draft goes on.
    let draft = setMovement(activate(newDraft(), "72"), [{ kind: "done_moving" }]);
    const asked = await run(draft);
    // AI Development Algorithm asks before the production. It is no step of the action.
    expect(asked.stop).toEqual({ kind: "open" });
    expect(asks(asked)).toBe("exhaust_for_production_discount");
    expect(frontier(asked)).toBeNull();
    draft = answer(draft, "invasion", "decline");

    const open = await run(draft);
    expect(open.stop).toEqual({ kind: "open" });
    expect(frontier(open)).toBe("production");
    expect(open.invasion).toBeUndefined();
    expect(stageAfter(open, "movement")).toBe("production");
    const facts = open.production?.tactical as ProductionFacts;
    expect(facts).toMatchObject({ kind: "production", system: "72" });
    const before = shipsIn(open.update, "72");

    // A cruiser that is not paid is kept in the draft, and the game is not asked for it.
    const unit = facts.units.findIndex((item) => item.unit === "cruiser2");
    let build = addBuild(facts, EMPTY, unit, "space");
    draft = setProduction(draft, productionSteps(facts, build), false);
    expect((await run(draft)).stop).toEqual({ kind: "open" });
    // Played unpaid, it is refused at its payment.
    const unpaid = (await ask([...scriptOf(entryOf(draft)), ...productionSteps(facts, build)]))!;
    // The refusal names the purchase that the payment does not cover.
    expect(unpaid.stop).toMatchObject({ kind: "refused", code: "unpaid" });
    expect(problemOf({ ...entryOf(draft), unpaid: false }, unpaid)).toBe(
      "The payment no longer covers the production: change the payment.",
    );

    // Lisis pays: the action is over, and the draft is the whole of it.
    build = togglePlanet(facts, build, "lisis");
    expect(totals(facts, build).paid).toBeGreaterThanOrEqual(totals(facts, build).cost);
    draft = setProduction(draft, productionSteps(facts, build), true);
    const done = await run(draft);
    expect(done.stop).toEqual({ kind: "complete" });
    expect(fits(entryOf(draft), done)).toBe(true);
    expect(shipsIn(done.update, "72")).toEqual([...before, "cruiser2"].sort());
    // The production is still staged against its first question.
    expect(done.production?.tactical).toEqual(facts);
    expect(productionOf(facts, entryOf(draft).production ?? [])).toEqual(build);

    // Nothing of this is in the game, until it is applied as one request.
    expect(answers()).toHaveLength(0);
    const script = scriptOf(entryOf(draft));
    transport.applyDraft(nonce(menu), {
      choices: ["tactical", "72"],
      plan: { destination: "72", steps: script.slice(2, done.consumed) as PlanStep[] },
    });
    const after = await next();
    expect(after.plan).toBeUndefined();
    expect(subtype(after)).not.toBe("produce_unit");
    expect(shipsIn(after.update, "72")).toEqual([...before, "cruiser2"].sort());
    expect(after.update.view.board.systems["72"].planets.lisis.exhausted).toBe(true);
    expect(answers().slice(0, 5)).toEqual([
      "tactical",
      "72",
      "done_moving",
      "decline",
      "build|cruiser2|1",
    ]);
    transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "has the printed exploration cards, and an update says what the decks hold",
  async () => {
    const printed = await loadCatalog({
      query: engineQuery(await WebAssembly.compile(readFileSync(WASM))),
    });
    const catalog = printed.exploration;
    expect(Object.keys(catalog.decks).sort()).toEqual([
      "CULTURAL",
      "FRONTIER",
      "HAZARDOUS",
      "INDUSTRIAL",
    ]);
    expect(catalog.cards.find((card) => card.id === "ds")).toMatchObject({
      name: "Dyson Sphere",
      deck: "CULTURAL",
      resolution: "Attach",
    });
    const { transport, next } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    const { update } = await next();
    transport.close();
    // The scenario drew nine cards. A deck names the cards that are left by id.
    const facts = update.exploration!;
    expect(facts.drawn).toHaveLength(9);
    expect(facts.decks.CULTURAL).toHaveLength(catalog.decks.CULTURAL.length - 5);
    expect(facts.decks.CULTURAL).not.toContain("ds");
    expect(facts.explores.abyz).toEqual(["HAZARDOUS"]);
    expect(facts.explores.vegamajor).toBeUndefined();
    expect(update.view.board.systems["74"].planets.vegamajor.attachments).toEqual(["dysonsphere"]);
    expect(update.view.players.find((player) => player.id === "a")!.relic_fragments).toEqual({
      CULTURAL: 3,
      HAZARDOUS: 1,
    });
    for (const id of Object.values(facts.decks).flat()) {
      expect(catalog.cards.some((card) => card.id === id)).toBe(true);
    }
  },
);

it.skipIf(!built)(
  "has the printed action cards, and an update says where the cards of the deck are",
  async () => {
    const catalog = await loadCatalog({
      query: engineQuery(await WebAssembly.compile(readFileSync(WASM))),
    });
    const deck = catalog.components.action_cards;
    const boosts = deck.filter((card) => card.name === "Morale Boost");
    expect(boosts).toHaveLength(4);
    expect(boosts[0]).toMatchObject({ phase: "Action", window: "At the start of a combat round" });
    const { transport, next } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    const { update } = await next();
    transport.close();
    const facts = update.action_cards!;
    expect(facts.hand_limit).toBe(7);
    // Every card of the deck is in the deck, in a hand or in the discard pile.
    const held = update.view.players.reduce((sum, player) => sum + player.action_cards_count, 0);
    expect(facts.deck + held + facts.discard.length).toBe(deck.length);
    const view = selectActionCards(update, catalog, { modes: {}, faked: [] })!;
    expect(view.held).toBe(view.hand.reduce((sum, card) => sum + card.copies, 0));
    for (const card of view.hand) {
      expect(card.text).not.toBe("");
      expect(card.window).toMatch(/:$/);
    }
    const rows = view.reference.flatMap((group) => group.cards);
    expect(rows.reduce((sum, card) => sum + card.total, 0)).toBe(deck.length);
    // The groups are the ones of the catalogue, in its order, and "ACTION:" is the first.
    const groups = view.reference.map((group) => group.group);
    expect(groups).toEqual(
      catalog.components.action_card_groups.filter((group) => groups.includes(group)),
    );
    expect(groups[0]).toBe("Action");
    expect(view.reference[0].cards.every((card) => card.action)).toBe(true);
    expect(rows.find((card) => card.name === "Sabotage")?.group).toBe("Any time");
  },
);

it.skipIf(!built)(
  "has the printed secret objectives, and an update says what each seat has scored",
  async () => {
    const catalog = await loadCatalog({
      query: engineQuery(await WebAssembly.compile(readFileSync(WASM))),
    });
    const deck = catalog.components.secret_objectives;
    expect(deck).toHaveLength(40);
    for (const card of deck) {
      expect(card.text).not.toBe("");
      expect(catalog.components.secret_objective_groups).toContain(card.group);
      expect(card.group).not.toBe("Other");
    }
    const { transport, next } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    const { update } = await next();
    transport.close();
    const facts = update.secret_objectives!;
    expect(facts.hand_limit).toBe(3);
    // Every objective is in the deck, in a hand or scored.
    const held = update.view.players.reduce(
      (sum, player) => sum + player.secret_objectives_count,
      0,
    );
    const scored = Object.values(facts.scored).flat();
    expect(facts.deck + held + scored.length).toBe(deck.length);
    const view = selectSecrets(update, catalog)!;
    expect(view.mine).toHaveLength(view.held + view.scored);
    for (const card of view.mine) {
      expect(card.text).not.toBe("");
      expect(card.phase).toMatch(/ phase$/);
    }
    const rows = view.reference.flatMap((group) => group.cards);
    expect(rows).toHaveLength(deck.length);
    expect(view.unseen).toBe(deck.length - view.held - scored.length);
    // The groups are the ones of the catalogue, in its order.
    const groups = view.reference.map((group) => group.group);
    expect(groups).toEqual(
      catalog.components.secret_objective_groups.filter((group) => groups.includes(group)),
    );
  },
);

it.skipIf(!built)(
  "has the printed agendas, and an update says which laws are in play",
  async () => {
    const catalog = await loadCatalog({
      query: engineQuery(await WebAssembly.compile(readFileSync(WASM))),
    });
    const deck = catalog.components.agendas!;
    expect(deck).toHaveLength(50);
    // Every agenda is in the deck once.
    expect(new Set(deck.map((card) => card.name)).size).toBe(50);
    for (const card of deck) {
      expect(card.text).not.toBe("");
      expect(["Law", "Directive"]).toContain(card.kind);
      expect(catalog.components.agenda_groups).toContain(card.group);
      expect(card.group).not.toBe("Other");
    }
    const { transport, next } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    const { update } = await next();
    transport.close();
    expect(update.view.table.laws).toEqual({ censure: "c", defense_act: "for" });
    // The two laws and Mutiny are not in the deck.
    expect(update.agendas).toEqual({ deck: 47 });
    const view = selectAgendas(update, catalog);
    expect(view.laws).toMatchObject([
      { name: "Homeland Defense Act", kind: "Law · For/Against", outcome: { text: "For" } },
      { name: "Political Censure", kind: "Law · Elect Player", outcome: { seat: "c" } },
    ]);
    expect(view.laws[0].text).toMatch(/^For: /);
    expect(view.rest).toBe(48);
    expect(view.reference.flatMap((group) => group.cards)).toHaveLength(48);
    // The groups are the ones of the catalogue, in its order.
    const groups = view.reference.map((group) => group.group);
    expect(groups).toEqual(
      catalog.components.agenda_groups!.filter((group) => groups.includes(group)),
    );
  },
);

it.skipIf(!built)("says where the points of every seat of the late game come from", async () => {
  const { transport, next } = await play(false, {
    seed: 42,
    players: 8,
    humans: 1,
    scenario: "late-game",
  });
  const { update } = await next();
  transport.close();
  const facts = update.score!;
  expect(facts.target).toBe(10);
  expect(facts.public.map((item) => item.points).sort()).toEqual([1, 1, 1, 1, 1, 2, 2]);
  for (const item of facts.public) {
    expect(item.name).not.toBe(item.id);
    expect(item.text).not.toBe("");
  }
  expect(facts.sources.map((source) => [source.kind, source.name])).toEqual([
    ["secrets", ""],
    ["custodians", ""],
    ["imperial", ""],
    ["support", ""],
    ["agenda", "Mutiny"],
    ["agenda", "Political Censure"],
    ["relic", "Shard of the Throne"],
  ]);
  expect(facts.sources[3].from).toEqual({ d: ["g"], f: ["c"] });
  expect(facts.unexplained).toBeUndefined();
  // The cells of a seat sum to its points.
  const view = selectScore(update, null)!;
  // The viewer (seat A, 5 points) is first, then the seats by their points.
  expect(view.seats.map((seat) => seat.points)).toEqual([5, 7, 6, 5, 5, 4, 4, 2]);
  const rows = view.groups.flatMap((group) => group.rows);
  view.seats.forEach((seat, index) => {
    const sum = rows.reduce((total, row) => {
      const cell = row.cells[index];
      return (
        total + (cell.kind === "scored" ? row.worth! : cell.kind === "points" ? cell.value : 0)
      );
    }, 0);
    expect(sum).toBe(seat.points);
  });
});

it.skipIf(!built)(
  "has the printed cards of a play area, and an update says which are used",
  async () => {
    const catalog = await loadCatalog({
      query: engineQuery(await WebAssembly.compile(readFileSync(WASM))),
    });
    const { transport, next } = await play(false, {
      seed: 42,
      players: 8,
      humans: 1,
      scenario: "late-game",
    });
    const { update } = await next();
    transport.close();
    const sol = selectPlayers(update, catalog).rows.find((row) => row.seat === "a")!;
    expect(sol.abilities.map((card) => [card.name, card.window, card.state])).toEqual([
      ["Orbital Drop", "ACTION:", null],
      ["Versatile", "When you gain command tokens during the status phase:", null],
    ]);
    expect(sol.abilities[0].text).toContain("2 infantry");
    expect(sol.leaders.map((card) => [card.name, card.kind, card.state])).toEqual([
      ["Evelyn DeLouis", "Agent · Master Tactician", "exhausted"],
      ["Claire Gibson", "Commander · Voice of the People", "ready"],
      ["Jace X, 4th Air Legion", "Hero · Federation Hero", "locked"],
    ]);
    expect(sol.leaders[0].unlock).toBe("");
    expect(sol.leaders[2].unlock).toBe("Have 3 scored objectives.");
    // Gravity Drive and the unit upgrades always hold: they are not cards of the sheet.
    expect(sol.components.map((card) => [card.name, card.kind, card.state])).toEqual([
      ["AI Development Algorithm", "Technology", "ready"],
      ["Bio-Stims", "Technology", "exhausted"],
      ["Dynamis Core", "Relic", "ready"],
      ["The Prophet's Tears", "Relic", "exhausted"],
    ]);
    expect(sol.technologies).toContain("GD");
    // The name and the commodity value of a faction, and the name of a strategy card, are
    // printed: every faction and every card set that a table of the server can play has them.
    const sheets = catalog.components.faction_sheets!;
    expect(Object.keys(sheets).length).toBeGreaterThanOrEqual(24);
    expect(sheets.ghost).toMatchObject({
      name: "Creuss",
      title: "The Ghosts of Creuss",
      commodities: 4,
    });
    // The sheet of a faction, as a player reads it before the game: all of it is printed.
    expect(factionSheet("sol", catalog)).toMatchObject({
      title: "The Federation of Sol",
      facts: [
        { label: "Home system", value: "Jord 4/2" },
        { label: "Commodities", value: "4" },
        { label: "Starting technologies", value: "Antimass Deflectors, Neural Motivator" },
        { label: "Starting fleet", value: "2 Advanced Carrier I · Destroyer I · 3 Fighter I" },
        { label: "On Jord", value: "5 Spec Ops I · Space Dock I" },
      ],
      abilities: [{ name: "Orbital Drop", window: "ACTION:" }, { name: "Versatile" }],
      technologies: [{ name: "Spec Ops II" }, { name: "Advanced Carrier II" }],
      leaders: [
        { name: "Evelyn DeLouis", kind: "Agent · Master Tactician" },
        { name: "Claire Gibson", kind: "Commander · Voice of the People" },
        { name: "Jace X, 4th Air Legion", unlock: "Have 3 scored objectives." },
      ],
    });
    expect(factionSheet("argent", catalog)!.facts[2].value).toBe(
      "Choose 2 of Neural Motivator, Sarween Tools, Plasma Scoring",
    );
    // Every faction of a lobby has a whole sheet, and no line of it is an id.
    for (const id of Object.keys(sheets)) {
      const sheet = factionSheet(id, catalog)!;
      const cards = [...sheet.abilities, ...sheet.technologies, ...sheet.leaders];
      expect(
        cards.every((item) => item.text !== ""),
        id,
      ).toBe(true);
      expect(
        sheet.facts.map((fact) => fact.label),
        id,
      ).toContain("Starting fleet");
    }
    expect(factionSheet("nobody", catalog)).toBeNull();
    expect(sol.commodities[1]).toBe(4);
    const cards = new Map(catalog.components.strategy_cards!.map((card) => [card.id, card.name]));
    expect(["te4construction", "te6warfare", "base4"].map((id) => cards.get(id))).toEqual([
      "Construction",
      "Warfare",
      "Construction",
    ]);
    // Without the catalogue the leaders keep their kind and their state.
    const bare = selectPlayers(update).rows.find((row) => row.seat === "a")!;
    expect(bare.leaders.map((card) => [card.name, card.state])).toEqual([
      ["Agent", "exhausted"],
      ["Commander", "ready"],
      ["Hero", "locked"],
    ]);
    expect(bare.abilities).toEqual([]);
  },
);

it.skipIf(!built)(
  "plays another seat from a decision on, several times, and a replay is the same game",
  async () => {
    const { transport, next, nonce, answers, save, switches } = await play();
    const seatOf = (event: Update) => {
      const { viewer, pending_choice } = event.update;
      expect(pending_choice?.choice.player).toBe(viewer.role === "player" ? viewer.seat : null);
      return pending_choice?.choice.player;
    };
    /** Answers the open choice by a fixed rule, and gives the update after it. */
    const answer = async (event: Update) => {
      const { options } = event.update.pending_choice!.choice;
      transport.submitChoice(nonce(event), options[(answers().length * 7) % options.length].id);
      return next();
    };
    let open = await next();
    for (let count = 0; count < 3; count += 1) {
      expect(seatOf(open)).toBe("a");
      open = await answer(open);
    }

    // Seat c is played here. The open decision of seat a is not an answer of the page.
    transport.capabilities.switchSeat?.("c");
    const first = await next();
    expect(seatOf(first)).toBe("c");
    expect(answers()).toHaveLength(3);
    // An undo does not give seat a back.
    expect(first.canUndo).toBe(false);
    transport.undo();
    expect(answers()).toHaveLength(3);
    open = await answer(first);
    expect(seatOf(open)).toBe("c");
    expect(open.canUndo).toBe(true);
    transport.undo();
    const again = await next();
    expect(again.update).toEqual(first.update);
    expect(again.canUndo).toBe(false);
    open = await answer(again);

    // Two changes with no answer between them, then back to the seat of the start.
    transport.capabilities.switchSeat?.("e");
    expect(seatOf(await next())).toBe("e");
    transport.capabilities.switchSeat?.("b");
    open = await next();
    expect(seatOf(open)).toBe("b");
    expect(open.canUndo).toBe(false);
    open = await answer(open);
    expect(seatOf(open)).toBe("b");
    // A seat that is played here already is no change.
    transport.capabilities.switchSeat?.("b");
    expect(switches()).toHaveLength(3);
    transport.capabilities.switchSeat?.("a");
    open = await next();
    expect(seatOf(open)).toBe("a");
    const reached = await answer(open);
    expect(seatOf(reached)).toBe("a");
    expect(switches().map((item) => item.humans)).toEqual([0b100, 0b10000, 0b10, 0b1]);
    // Each is at a later decision of the game.
    const at = switches().map((item) => item.decision);
    expect(at).toEqual([...new Set(at)].sort((left, right) => left - right));
    transport.close();

    // A new visit: the decisions and the seat changes play the same game.
    const visit = await play(false, undefined, { save: save() });
    const resumed = await visit.next();
    expect(resumed.update).toEqual(reached.update);
    expect(resumed.canUndo).toBe(true);
    // The undo of a new visit plays the game again, with its seat changes.
    visit.transport.undo();
    const back = await visit.next();
    expect(back.update).toEqual(open.update);
    expect(back.canUndo).toBe(false);
    visit.transport.close();
  },
);

it.skipIf(!built)(
  "takes the seat that the game waits for, while the seats are stepped",
  async () => {
    const { transport, next, nonce, save } = await play(true);
    const menu = await next();
    transport.submitChoice(nonce(menu), menu.update.pending_choice!.choice.options[0].id);
    const paused = await next();
    const seat = paused.stepping!.seat;
    expect(seat).not.toBe("a");
    expect(paused.update.pending_choice).toBeUndefined();
    // The decision that the game waited to draw is asked here.
    transport.capabilities.switchSeat?.(seat);
    const asked = await next();
    expect(asked.stepping).toBeUndefined();
    expect(asked.update.viewer).toEqual({ role: "player", seat });
    expect(asked.update.pending_choice?.choice.player).toBe(seat);
    expect(asked.canUndo).toBe(false);
    transport.close();

    // Played again without steps, the change is at the same decision.
    const visit = await play(false, undefined, { save: save() });
    expect((await visit.next()).update).toEqual(asked.update);
    visit.transport.close();
  },
);

// In the scenario "Late game" seat d (Xxcha) has the turn first of its seats and holds Nav Suite,
// which a window offers after the seat activates a system. The game opens that window twice.
const XXCHA: LocalGame = { seed: 42, players: 8, humans: 0b1000, scenario: "late-game" };
const NAV_SUITE = "Nav Suite";

it.skipIf(!built)(
  "asks a window of a card, and passes it in the engine once the card is never offered",
  async () => {
    const { transport, next, nonce, answers, passed, never } = await play(false, XXCHA);
    expect(never()).toEqual([]);
    transport.submitChoice(nonce(await next()), "tactical");
    transport.submitChoice(nonce(await next()), "41");
    const window = await next();
    expect(subtype(window)).toBe("reaction_after_SYSTEM_ACTIVATED");
    expect(window.update.pending_choice?.choice.options.map((option) => option.kind)).toEqual([
      "ability",
      "decline",
    ]);
    // The card is set while its window is open: that window stays, and the player answers it.
    transport.setReactionMode(NAV_SUITE, "never");
    expect(never()).toEqual([NAV_SUITE]);
    expect(passed).toEqual([]);
    transport.submitChoice(nonce(window), "decline");
    // The window after it is passed in the engine.
    const moving = await next();
    expect(subtype(moving)).toBe("movement_step");
    expect(passed).toEqual([[NAV_SUITE]]);
    // The decline is an answer of the game: a saved game has it.
    expect(answers()).toEqual(["tactical", "41", "decline", "decline"]);
    transport.close();
  },
  60_000,
);

// In the scenario "Late game" seat g is Sardakk: it holds Reverse Engineer, which a window offers
// when another seat discards a card, and its agent T'ro is readied, which a window offers at the
// end of the tactical action of any seat.
const SARDAKK: LocalGame = { seed: 42, players: 8, humans: 0b1000000, scenario: "late-game" };
const TRO = "leader:sardakkagent";

/** Answers what the seat is asked with a pass, up to the window that offers `key`. */
async function toWindowOf(game: Awaited<ReturnType<typeof play>>, key: string): Promise<Update> {
  for (let asked = 0; asked < 200; asked++) {
    const event = await game.next();
    if (event.update.reaction?.offers.some((offer) => offer.key === key)) {
      return event;
    }
    const { options } = event.update.pending_choice!.choice;
    const pass =
      options.find((option) => option.kind === "decline") ??
      options.find((option) => option.id === "pass") ??
      options[options.length - 1];
    game.transport.submitChoice(game.nonce(event), pass.id);
  }
  throw new Error(`no window offered ${key}`);
}

it.skipIf(!built)(
  "says what a window of an ability offers as printed, and passes it for a seat that wants it in its turns only",
  async () => {
    const catalog = await loadCatalog({
      query: engineQuery(await WebAssembly.compile(readFileSync(WASM))),
    });
    const game = await play(false, SARDAKK);
    const window = await toWindowOf(game, TRO);
    const { update } = window;
    const choice = update.pending_choice!.choice;
    expect(subtype(window)).toBe("reaction_after_TACTICAL_ACTION_ENDED");
    // The window is in the action phase, in the turn of another seat.
    const active = choice.details?.active;
    expect(active).not.toBe("g");
    expect(update.reaction).toMatchObject({
      own_turn_applies: true,
      trigger: { event_type: "TACTICAL_ACTION_ENDED", relation: "after", actor: active },
    });
    const offer = update.reaction!.offers.find((item) => item.key === TRO)!;
    expect(offer).toMatchObject({ kind: "leader", role: "agent", name: "T'ro" });
    expect(offer.window).toMatch(/^At the end of a player's tactical action/);
    expect(offer.text).toMatch(/exhaust this card/);

    // The screen: the trigger in game values, the card as printed, the setting, and no event id.
    const view = selectReaction(update, choice, null, catalog, game.modes())!;
    const faction = factionName(
      update.view.players.find((player) => player.id === active)!.faction,
      catalog,
    );
    expect(view.heading).toBe(
      `After ${faction}’s tactical action in system ${update.reaction!.trigger!.system}`,
    );
    expect(view.pass).toBe("decline");
    const row = view.rows.find((item) => item.offer?.key === TRO)!;
    expect(row).toMatchObject({
      key: "1",
      title: "T'ro",
      state: "Agent",
      offer: { name: "T'ro", mode: "always", ownTurn: true },
      intent: { type: "chooseOption", option: offer.option },
    });
    expect(row.text).toMatch(
      /^At the end of a player's tactical action: You may exhaust this card/,
    );
    expect(JSON.stringify(view)).not.toContain("TACTICAL_ACTION_ENDED");
    // The decline is the main button, and no row.
    expect(view.rows).toHaveLength(choice.options.length - 1);

    // Set while its window is open: that window stays, and the player answers it.
    const before = game.passed.length;
    game.transport.setReactionMode(TRO, "ownturn");
    expect(game.modes()).toEqual({ [TRO]: "ownturn" });
    expect(game.passed).toHaveLength(before);
    game.transport.submitChoice(game.nonce(window), "decline");
    const after = await game.next();
    expect(after.update.pending_choice?.nonce).not.toBe(update.pending_choice?.nonce);
    game.transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "offers a reaction in the turn of its seat when the seat wants it in its turns only",
  async () => {
    // Nav Suite is offered after seat d activates a system: its own turn.
    const game = await play(false, XXCHA, { modes: { [NAV_SUITE]: "ownturn" } });
    game.transport.submitChoice(game.nonce(await game.next()), "tactical");
    game.transport.submitChoice(game.nonce(await game.next()), "41");
    const window = await game.next();
    expect(subtype(window)).toBe("reaction_after_SYSTEM_ACTIVATED");
    expect(window.update.pending_choice?.choice.details?.active).toBe("d");
    expect(window.update.reaction?.offers.map((offer) => offer.key)).toEqual([NAV_SUITE]);
    expect(game.passed).toEqual([]);
    game.transport.close();

    // In the turn of another seat the same mode passes the window of a card.
    const other = await play(false, SARDAKK, { modes: { "Reverse Engineer": "ownturn" } });
    const asked = await other.next();
    expect(
      asked.update.reaction?.offers.some((offer) => offer.key === "Reverse Engineer") ?? false,
    ).toBe(false);
    expect(other.passed[0]).toEqual(["Reverse Engineer"]);
    other.transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "takes a passed window back with the answer before it, and plays a saved game whatever the cards are set to",
  async () => {
    const first = await play(false, XXCHA, { never: [NAV_SUITE] });
    expect(first.never()).toEqual([NAV_SUITE]);
    first.transport.submitChoice(first.nonce(await first.next()), "tactical");
    first.transport.submitChoice(first.nonce(await first.next()), "41");
    // The window does not open.
    const moving = await first.next();
    expect(subtype(moving)).toBe("movement_step");
    expect(first.passed).toEqual([[NAV_SUITE], [NAV_SUITE]]);
    expect(first.answers()).toEqual(["tactical", "41", "decline", "decline"]);
    expect(moving.canUndo).toBe(true);

    // One undo goes back over the windows, to the activation. Nothing is said again.
    first.transport.undo();
    const activation = await first.next();
    expect(subtype(activation)).toBe("activate_system");
    expect(first.answers()).toEqual(["tactical"]);
    expect(first.passed).toHaveLength(2);
    // The same answer passes the windows again.
    first.transport.submitChoice(first.nonce(activation), "41");
    expect(subtype(await first.next())).toBe("movement_step");
    expect(first.passed).toHaveLength(4);
    const saved = first.save();
    first.transport.close();

    // The card is offered again, and the saved game is the same game: its window was declined.
    const offered = await play(false, XXCHA, { save: saved });
    expect(subtype(await offered.next())).toBe("movement_step");
    expect(offered.passed).toEqual([]);
    offered.transport.close();

    // A game where the player declined the window, played again with the card never offered.
    const again = await play(false, XXCHA, { save: saved, never: [NAV_SUITE] });
    const resumed = await again.next();
    expect(subtype(resumed)).toBe("movement_step");
    expect(again.passed).toEqual([]);
    // An undo opens a window, which is passed now, and goes on to the activation.
    again.transport.undo();
    expect(subtype(await again.next())).toBe("activate_system");
    expect(again.answers()).toEqual(["tactical"]);
    again.transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "stops a plan at a window of a card, and not at a window that is passed",
  async () => {
    const plan = { destination: "41", steps: [{ kind: "done_moving" } as const] };
    const asked = await play(false, XXCHA);
    const menu = await asked.next();
    asked.transport.applyDraft(asked.nonce(menu), { choices: ["tactical", "41"], plan });
    const window = await asked.next();
    expect(subtype(window)).toBe("reaction_after_SYSTEM_ACTIVATED");
    expect(window.plan).toMatchObject({ applied: 0, interrupted: true, remaining: plan.steps });
    asked.transport.close();

    const never = await play(false, XXCHA, { never: [NAV_SUITE] });
    never.transport.applyDraft(never.nonce(await never.next()), {
      choices: ["tactical", "41"],
      plan,
    });
    const after = await never.next();
    expect(after.plan).toBeUndefined();
    expect(subtype(after)).not.toBe("movement_step");
    expect(never.answers().slice(0, 5)).toEqual([
      "tactical",
      "41",
      "decline",
      "decline",
      "done_moving",
    ]);
    // An undo takes the movement back, and the next one the activation with its windows.
    never.transport.undo();
    expect(subtype(await never.next())).toBe("movement_step");
    expect(never.answers()).toEqual(["tactical", "41", "decline", "decline"]);
    never.transport.undo();
    expect(subtype(await never.next())).toBe("activate_system");
    expect(never.answers()).toEqual(["tactical"]);
    never.transport.close();
  },
  60_000,
);

it.skipIf(!built)(
  "drafts a secondary from the moment its card is played, and sends it when the seat is asked",
  async () => {
    const { transport, next, nonce, draft } = await play(true);
    const choose = (option_id: string): DraftStep => ({ kind: "choose", option_id });
    // Seat a takes the last card, so that another seat plays a strategy card before its turn.
    let event = await next();
    let drafted: { scope: PlanScope; pass: string } | null = null;
    for (let turns = 0; turns < 600 && !drafted; turns += 1) {
      const action = event.update.secondary;
      if (event.update.pending_choice) {
        const { choice } = event.update.pending_choice;
        // Its own decisions before that: the card, and a pass of whatever is asked.
        const option =
          choice.options.find((item) => item.id === "pok8imperial") ?? choice.options[0];
        transport.submitChoice(nonce(event), option.id);
      } else if (action?.draft) {
        // Another seat decides, and the card is certain: the draft is asked on the game as it
        // was before the card was played.
        expect(action.primary).not.toBe("a");
        expect(action.followers.find((follower) => follower.seat === "a")?.state).toBe("waiting");
        const scope: PlanScope = { kind: "secondary", primary: action.primary, card: action.card };
        const asked = await draft([], scope);
        if (asked?.stop.kind === "complete") {
          // A secondary that the seat cannot use asks nothing (Leadership with no influence).
          expect(asked.consumed).toBe(0);
          transport.capabilities.stepping?.step();
          event = await next();
          continue;
        }
        expect(asked?.stop).toEqual({ kind: "open" });
        const question = asked!.update.pending_choice!.choice;
        expect(question.player).toBe("a");
        expect(question.details).toMatchObject({
          kind: "strategy_secondary",
          card: action.card,
          played_by: action.primary,
        });
        // The draft of another card is refused, and so is a draft with no card played.
        const other = await draft([], { ...scope, card: "pok1leadership" });
        expect(other?.stop.kind).toBe("failed");
        const pass = question.options[0].id;
        const passed = await draft([choose(pass)], scope);
        expect(passed?.stop).toEqual({ kind: "complete" });
        expect(passed?.consumed).toBe(1);
        drafted = { scope, pass };
        transport.capabilities.stepping?.set(false);
      } else {
        transport.capabilities.stepping?.step();
      }
      event = await next();
    }
    expect(drafted).not.toBeNull();
    // The game asks seat a for its secondary: the draft is sent as its answer.
    const action = event.update.secondary!;
    expect(action.open).toBe(true);
    expect(event.update.pending_choice?.choice.details).toMatchObject({
      kind: "strategy_secondary",
      card: drafted!.scope.kind === "secondary" ? drafted!.scope.card : "",
    });
    const again = await draft([choose(drafted!.pass)], drafted!.scope);
    expect(again?.stop).toEqual({ kind: "complete" });
    transport.applyDraft(nonce(event), { choices: [drafted!.pass], plan: null });
    const after = await next();
    expect(after.plan).toBeUndefined();
    // The window goes on with the next seat, or the game is in another action by now.
    const same = after.update.secondary?.card === action.card;
    const mine = after.update.secondary?.followers.find((follower) => follower.seat === "a");
    expect(same ? mine?.state : "passed").toBe("passed");
    transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "the rule bot plays the seats of its mask, and the same mask is the same game",
  async () => {
    const game: LocalGame = { seed: 3, players: 6, humans: 1 };
    /** The first choices of seat a, each answered with its first option. */
    const first = async (ruleSeats: number) => {
      const { transport, next, nonce } = await play(false, game, { ruleSeats });
      const asked: string[] = [];
      for (let count = 0; count < 6; count++) {
        const event = await next();
        const choice = event.update.pending_choice!.choice;
        // What the other seats did is in the view: the questions of seat a alone can be the same.
        asked.push(
          `${choice.prompt}: ${choice.options.map((option) => option.id).join(",")} ${JSON.stringify(event.update.view.players)}`,
        );
        transport.submitChoice(nonce(event), choice.options[0].id);
      }
      transport.close();
      return asked;
    };
    const ruled = await first(0b111110);
    expect(await first(0b111110)).toEqual(ruled);
    // Random seats play another game: the mask reached the engine.
    expect(await first(0)).not.toEqual(ruled);
  },
  120_000,
);

it.skipIf(!built)(
  "sets up a table: its options, its board, its draw, and the game of that draw",
  async () => {
    const module = await WebAssembly.compile(readFileSync(WASM));
    const options = await setupOptionsOf(module, 4);
    expect(options.strategy_card_sets.map((set) => set.id)).toEqual([
      "te",
      "pok",
      "base_game_codex1",
    ]);
    expect(options.bot_kinds.map((kind) => kind.id)).toEqual(["smart", "random"]);
    expect(options.bot_kinds[0].description).toMatch(/^The page plays/);
    expect(options.faction_groups.flatMap((group) => group.factions)).toHaveLength(24);
    expect(options.maps.length).toBeGreaterThan(0);
    expect(options.maps.every((map) => map.player_count === 4 && map.buildable)).toBe(true);
    const usual = options.maps.find((map) => map.recommended)!;

    const table: TableRequest = {
      setup: {
        player_count: 4,
        seed: 12,
        map: { kind: "template", alias: usual.alias },
        strategy_cards: "pok",
      },
      seats: [
        { kind: "human", faction: "hacan" },
        { kind: "smart" },
        { kind: "random", faction: "ghost" },
        { kind: "smart" },
      ],
    };
    const preview = await setupPreviewOf(module, table);
    expect(preview.map.alias).toBe(usual.alias);
    expect(preview.seats).toHaveLength(4);
    // A place of a seat is a tile of the board, with no planets and no faction.
    for (const seat of preview.seats) {
      const tile = preview.tiles.find((have) => have.system_id === seat.home_system_id);
      expect(tile).toBeDefined();
      expect(tile?.planets ?? []).toEqual([]);
    }

    const draw = await setupDrawOf(module, table);
    expect([...draw.players].sort()).toEqual(["a", "b", "c", "d"]);
    expect(draw.factions[draw.players.indexOf("a")]).toBe("hacan");
    expect(draw.factions[draw.players.indexOf("c")]).toBe("ghost");
    expect(new Set(draw.factions).size).toBe(4);

    // The game is that draw: the page is player a, with the faction that it chose.
    const { transport, next } = await play(false, { seed: 12, players: 4, humans: 1 }, { table });
    const { update } = await next();
    expect(update.pending_choice?.choice.player).toBe("a");
    expect(update.view.players.map((player) => player.id)).toEqual(draw.players);
    expect(update.view.players.map((player) => player.faction)).toEqual(draw.factions);
    transport.close();

    // A table that cannot be is refused with the reason.
    await expect(
      setupDrawOf(module, { ...table, setup: { ...table.setup, strategy_cards: "base_game" } }),
    ).rejects.toThrow(/strategy_card_set/);
  },
  120_000,
);

it.skipIf(!built)(
  "the wait of a table: the page is shown the game while a seat takes its time, and the game is the same",
  async () => {
    const table: TableRequest = {
      setup: { player_count: 4, seed: 12 },
      seats: [{ kind: "human" }, { kind: "smart" }, { kind: "random" }, { kind: "smart" }],
    };
    /** The first choices of player a, each answered with its first option, and the waits before them. */
    const first = async (waitMs: number) => {
      const { transport, next, nonce } = await play(
        false,
        { seed: 12, players: 4, humans: 1 },
        { table, waitMs },
      );
      const asked: string[] = [];
      const waited: string[] = [];
      while (asked.length < 4) {
        const event = await next();
        const pending = event.update.pending_choice;
        if (!pending) {
          // The game waits for another seat: the status names it, and nothing is to step.
          expect(event.stepping).toBeUndefined();
          const turn = event.update.turn_status;
          waited.push(turn.kind === "waiting_for_decision" ? turn.seat : turn.kind);
          continue;
        }
        const { choice } = pending;
        asked.push(
          `${choice.prompt}: ${choice.options.map((option) => option.id).join(",")} ${JSON.stringify(event.update.view.players)}`,
        );
        transport.submitChoice(nonce(event), choice.options[0].id);
      }
      transport.close();
      return { asked, waited };
    };
    const now = await first(0);
    expect(now.waited).toEqual([]);
    const paced = await first(2);
    expect(paced.asked).toEqual(now.asked);
    expect(paced.waited.length).toBeGreaterThan(0);
    expect(paced.waited).not.toContain("a");
  },
  120_000,
);

it.skipIf(!built)(
  "a saved game has the decisions of every seat: a new visit goes on where the game was, and an undo takes them back with the answer",
  async () => {
    const table: TableRequest = {
      setup: { player_count: 4, seed: 12 },
      seats: [{ kind: "human" }, { kind: "smart" }, { kind: "random" }, { kind: "smart" }],
    };
    const game = { seed: 12, players: 4, humans: 1 };
    const first = await play(true, game, { table });
    // Player a answers, and the other seats are stepped, until two of them decided after the
    // last answer and a third is waited for.
    let event = await first.next();
    let asked = event;
    let before = -1;
    let stepped = 0;
    while (event.update.pending_choice || stepped < 2 || before < 0) {
      if (event.update.pending_choice) {
        asked = event;
        before = first.decisions().length;
        stepped = 0;
        const [option] = event.update.pending_choice.choice.options;
        first.transport.submitChoice(first.nonce(event), option.id);
      } else {
        stepped += 1;
        first.transport.capabilities.stepping?.step();
      }
      event = await first.next();
    }
    const answers = first.answers();
    const saved = first.decisions();
    const kept = first.save();
    // The decisions of the other seats are in the game, in their order.
    expect(saved).toHaveLength(before + 3);
    expect(saved[before]).toBe(answers.at(-1));
    expect(saved.length).toBeGreaterThan(answers.length + 2);
    first.transport.close();

    // A new visit, stepped: it waits for the same seat, on the same game.
    const visit = await play(true, game, { table, save: kept });
    const resumed = await visit.next();
    expect(resumed.stepping).toEqual(event.stepping);
    expect(resumed.update).toEqual(event.update);
    expect(visit.answers()).toEqual(answers);
    // An undo opens the last answer of the page again: what the other seats decided after it
    // is gone with it.
    visit.transport.undo();
    const back = await visit.next();
    expect(back.update).toEqual(asked.update);
    expect(visit.decisions()).toEqual(saved.slice(0, before));
    expect(visit.answers()).toEqual(answers.slice(0, -1));
    visit.transport.close();

    // A new visit with the wait of the table: the page is shown that seat at once.
    const paced = await play(false, game, { table, waitMs: 2, save: kept });
    const waiting = await paced.next();
    expect(waiting.stepping).toBeUndefined();
    expect(waiting.update).toEqual(event.update);
    paced.transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "the game of a table says who sits in each seat, and its wait changes while it runs",
  async () => {
    const seats: TableRequest["seats"] = [
      { kind: "human", name: "Ada" },
      { kind: "smart", name: "Rule bot 1" },
      { kind: "random", name: "Random 1" },
      { kind: "smart", name: "Rule bot 2" },
    ];
    const table: TableRequest = { setup: { player_count: 4, seed: 12 }, seats };
    const game = { seed: 12, players: 4, humans: 1 };
    const { transport, next, nonce } = await play(false, game, { table });
    let event = await next();
    // The players have the ids of the join ("a" came first), wherever the draw seated them.
    expect(event.update.seats).toEqual({
      a: { nickname: "Ada", kind: "human" },
      b: { nickname: "Rule bot 1", kind: "smart" },
      c: { nickname: "Random 1", kind: "random" },
      d: { nickname: "Rule bot 2", kind: "smart" },
    });
    // With no wait the page is asked only its own decisions. The host sets a wait: from now on
    // the page is shown the game while another seat takes its time.
    expect(event.update.pending_choice).toBeDefined();
    transport.capabilities.wait?.(2);
    let waited = 0;
    for (let asked = 0; asked < 6 && waited === 0;) {
      if (event.update.pending_choice) {
        asked += 1;
        transport.submitChoice(nonce(event), event.update.pending_choice.choice.options[0].id);
      } else {
        waited += 1;
        expect(event.update.seats?.a.nickname).toBe("Ada");
      }
      event = await next();
    }
    expect(waited).toBeGreaterThan(0);
    transport.close();

    // A table of an older page has no names: its game says none.
    const old = await play(false, game, {
      table: { ...table, seats: seats.map(({ kind }) => ({ kind })) },
    });
    expect((await old.next()).update.seats).toBeUndefined();
    old.transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "a model of the table is loaded in the Worker before the game, and a seat of it plays",
  async () => {
    const dir = fileURLToPath(new URL("../../../fixtures/models", import.meta.url));
    const index = JSON.parse(readFileSync(`${dir}/index.json`, "utf8")) as {
      models: { name: string; label: string; sha256: string }[];
    };
    const model = index.models[0];
    const file = (): ModelFile => ({
      sha256: model.sha256,
      bytes: new Uint8Array(readFileSync(`${dir}/${model.name}.ti4m`)).buffer,
    });
    const table: TableRequest = {
      setup: { player_count: 3, seed: 12 },
      seats: [
        { kind: "human", name: "Ada" },
        { kind: `mlp:${model.name}`, name: `${model.label} 1` },
        { kind: "smart", name: "Rule bot 1" },
      ],
    };
    const game = { seed: 12, players: 3, humans: 1 };
    const { transport, next } = await play(false, game, { table, models: [file()] });
    const event = await next();
    expect(event.update.seats?.b).toEqual({
      nickname: `${model.label} 1`,
      kind: `mlp:${model.name}`,
    });
    expect(event.update.pending_choice).toBeDefined();
    transport.close();

    // Without the file the engine refuses the game, with the name of the model.
    const errors: string[] = [];
    const bare = createWasmTransport(
      {
        module: await WebAssembly.compile(readFileSync(WASM)),
        hash: "test",
        scenarios: [],
        catalog: null,
      },
      { ...game, table },
    );
    bare.subscribe((each) => each.kind === "error" && errors.push(each.message));
    await expect.poll(() => errors.join()).toContain(model.name);
    bare.close();
  },
  120_000,
);

it.skipIf(!built)(
  "tells the public events of a local game with each update, and an undo takes them back",
  async () => {
    const game = await play();
    const first = await game.next();
    const picks = (event: Update) =>
      (event.events ?? []).filter((entry) => entry.fact?.kind === "strategy_picked");
    expect(picks(first)).toEqual([]);
    const option = first.update.pending_choice!.choice.options[0].id;
    game.transport.submitChoice(game.nonce(first), option);
    let after = await game.next();
    while (!picks(after).some((entry) => entry.actor === "a")) {
      after = await game.next();
    }
    expect(after.events!.every((entry) => entry.visibility.visibility === "public")).toBe(true);
    expect(new Set(after.events!.map((entry) => entry.id)).size).toBe(after.events!.length);
    game.transport.undo();
    let back = await game.next();
    while (back.update.pending_choice?.choice.context?.subtype !== subtype(first)) {
      back = await game.next();
    }
    expect(picks(back).some((entry) => entry.actor === "a")).toBe(false);
  },
);
