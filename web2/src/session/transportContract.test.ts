import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCatalog } from "@ti4/shared/content/catalog.ts";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  type TestServer,
  spawnTestServer,
  testServerBinary,
} from "../../../web-shared/test/server.ts";
import { canMove, setLoad, setShip, shipId } from "./movementDraft";
import { type PlanStep, stepsOf } from "./movementPlan";
import type { Transport, TransportEvent } from "./transport";
import { type Engine, createWasmTransport, engineQuery } from "./wasmTransport";
import type {
  ActivationFacts,
  DraftOutcome,
  DraftStep,
  MovementFacts,
  PlanScope,
  ReactionModes,
  SessionUpdate,
} from "./wire";
import { createWsTransport, openSocket } from "./wsTransport";

// What a transport promises, checked on both: the engine in this page, and a real server. The
// page above a transport is one code, so this is what it relies on. The two games are not the
// same game (the server draws its own map), so a case finds its system and its ship in the facts.
// Not here, because only one has it: the steps, the seat changes, the saved game, and how far
// an undo goes (a local game has no barrier, the server ends at one).
//
// Both are built outside this suite, and a case is skipped without its build:
// `scripts/build-wasm.sh`, and `cargo b-web`.
const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));
const SERVER = testServerBinary();

type Update = Extract<TransportEvent, { kind: "update" }>;

/** One game on one transport, as a test plays it. */
interface Side {
  transport: Transport;
  /** The next event that is an update and that `wanted` holds for. Updates before it are passed. */
  until(wanted: (event: Update) => boolean): Promise<Update>;
  draft(script: DraftStep[], scope?: PlanScope): Promise<DraftOutcome | null>;
  never(): string[];
  modes(): ReactionModes;
  /** Waits until the transport told this mode of a reaction. */
  mode(key: string, wanted: "always" | "never" | "ownturn"): Promise<void>;
  /** Waits until the transport told the Never cards as `wanted`. */
  cards(wanted: string[]): Promise<void>;
}

function side(transport: Transport): Side {
  const seen: Update[] = [];
  const drafts = new Map<DraftStep[], (outcome: DraftOutcome | null) => void>();
  let never: string[] = [];
  let modes: ReactionModes = {};
  let failed: string | null = null;
  let wake: (() => void) | null = null;
  transport.subscribe((event) => {
    if (event.kind === "error") {
      failed = event.message;
    } else if (event.kind === "update") {
      seen.push(event);
    } else if (event.kind === "draft") {
      drafts.get(event.script)?.(event.outcome);
      drafts.delete(event.script);
    } else if (event.kind === "modes") {
      never = Object.keys(event.modes).filter((key) => event.modes[key] === "never");
      modes = event.modes;
    }
    wake?.();
  });
  const wait = async (ready: () => boolean) => {
    while (!ready()) {
      if (failed !== null) {
        throw new Error(failed);
      }
      await new Promise<void>((resume) => {
        wake = resume;
      });
    }
  };
  let read = 0;
  return {
    transport,
    async until(wanted) {
      for (;;) {
        await wait(() => seen.length > read);
        const event = seen[read++];
        if (wanted(event)) {
          return event;
        }
      }
    },
    draft: (script, scope) =>
      new Promise((answer) => {
        drafts.set(script, answer);
        transport.draft(script, scope);
      }),
    never: () => never,
    modes: () => modes,
    cards: (wanted) => wait(() => never.join() === wanted.join()),
    mode: (key, wanted) => wait(() => (modes[key] ?? "always") === wanted),
  };
}

const choiceOf = (event: Update) => event.update.pending_choice?.choice;
const nonce = (event: Update) => event.update.pending_choice!.nonce;
const subtype = (event: Update) => choiceOf(event)?.context?.subtype;
const seat = (update: SessionUpdate) => (update.viewer.role === "player" ? update.viewer.seat : "");
/** The units of the viewer in the space area of a system. */
const shipsIn = (update: SessionUpdate, system: string) =>
  (update.view.board.systems[system]?.units ?? [])
    .filter((unit) => unit.owner === seat(update) && unit.planet === null)
    .map((unit) => unit.unit_type)
    .sort();

/** The next choice of this side that is not the one with `answered`. */
const asked = (game: Side, answered?: string) =>
  game.until((event) => !!choiceOf(event) && nonce(event) !== answered);

/** Answers as a seat that only wants the game to go on, up to the choice of `wanted`. */
async function playTo(game: Side, wanted: string): Promise<Update> {
  let answered: string | undefined;
  for (;;) {
    const event = await asked(game, answered);
    if (subtype(event) === wanted) {
      return event;
    }
    const { options } = choiceOf(event)!;
    const option = options.find((item) => item.kind === "decline") ?? options[0];
    answered = nonce(event);
    game.transport.submitChoice(answered, option.id);
  }
}

/**
 * A system that ships of the seat can move to, with a planet, no units in it and no production
 * of the seat: a ship that moves there stays there, and its ground forces can land.
 */
const target = (update: SessionUpdate) =>
  (update.tactical as ActivationFacts).systems
    .filter(
      (system) =>
        system.ships > 0 &&
        !system.production &&
        !update.view.board.systems[system.system]?.units.length &&
        update.view.board.map_tiles?.find((tile) => tile.system_id === system.system)?.planets
          ?.length,
    )
    .map((system) => system.system)
    .sort()[0];

/** The steps that move the first ship that can move, with an empty hold. */
function oneShip(facts: MovementFacts) {
  const ship = facts.ships.find((item) => canMove(facts, {}, item))!;
  return { ship, steps: stepsOf(facts, setShip(facts, {}, shipId(ship), true)) as PlanStep[] };
}

/** The game at the movement of a tactical action of the viewer, in the first turn of the seat. */
async function toMovement(game: Side) {
  const menu = await playTo(game, "action_menu");
  game.transport.submitChoice(nonce(menu), "tactical");
  const activation = await asked(game, nonce(menu));
  const system = target(activation.update);
  game.transport.submitChoice(nonce(activation), system);
  const open = await playTo(game, "movement_step");
  return { system, open, facts: open.update.tactical as MovementFacts };
}

function contract(start: () => Promise<Side>) {
  it("tells the game again after an answer, with the facts of the choice", async () => {
    const game = await start();
    const menu = await playTo(game, "action_menu");
    const viewer = seat(menu.update);
    expect(viewer).not.toBe("");
    expect(choiceOf(menu)?.player).toBe(viewer);
    // The facts that the sheets read come with every update.
    for (const fact of ["exploration", "action_cards", "secret_objectives", "score", "agendas"]) {
      expect(menu.update, fact).toHaveProperty(fact);
    }
    game.transport.submitChoice(nonce(menu), "tactical");
    const activation = await asked(game, nonce(menu));
    expect(subtype(activation)).toBe("activate_system");
    expect(activation.update.tactical?.kind).toBe("activation");
    const system = target(activation.update);
    expect(system).toBeDefined();
    // An answer to a choice that is not open is ignored.
    game.transport.submitChoice(nonce(menu), "pass");
    game.transport.submitChoice(nonce(activation), system);
    const next = await asked(game, nonce(activation));
    expect(next.update.view.board.active_system).toBe(system);
    game.transport.close();
  }, 60_000);

  it("takes a refused plan back whole: the same choice is open", async () => {
    const game = await start();
    const { system, open, facts } = await toMovement(game);
    expect(facts.moved).toBe(0);
    const { ship, steps } = oneShip(facts);
    // A step that the game does not offer, after a ship that it does offer.
    const bad: PlanStep[] = [
      ...steps.slice(0, -1),
      { ...(steps[0] as PlanStep & { kind: "move" }), unit: "no_such_ship" },
      steps.at(-1)!,
    ];
    game.transport.submitPlan(nonce(open), { destination: system, steps: bad });
    const refused = await game.until(() => true);
    expect(refused.plan).toMatchObject({
      destination: system,
      applied: 0,
      remaining: bad,
      interrupted: false,
    });
    expect(refused.plan?.reason).not.toBe("");
    expect(nonce(refused)).toBe(nonce(open));
    expect(refused.update.tactical).toEqual(facts);
    expect(shipsIn(refused.update, system)).toEqual(shipsIn(open.update, system));

    // The plan that fits is taken, and nothing is told about it.
    game.transport.submitPlan(nonce(refused), { destination: system, steps });
    const after = await game.until((event) => shipsIn(event.update, system).includes(ship.unit));
    expect(after.plan).toBeUndefined();
    game.transport.close();
  }, 60_000);

  it("takes a movement back whole with one undo, and tells its steps", async () => {
    const game = await start();
    const { system, open, facts } = await toMovement(game);
    const { ship, steps } = oneShip(facts);
    game.transport.submitPlan(nonce(open), { destination: system, steps });
    const moved = await game.until(
      (event) => !!choiceOf(event) && shipsIn(event.update, system).includes(ship.unit),
    );
    expect(moved.canUndo).toBe(true);
    game.transport.undo();
    const back = await game.until((event) => !!event.undone);
    // The movement is open again with nothing moved, and its steps are the ones that were sent
    // (the game does not ask to close a hold that closed itself).
    expect(subtype(back)).toBe("movement_step");
    expect((back.update.tactical as MovementFacts).moved).toBe(0);
    expect(shipsIn(back.update, system)).toEqual(shipsIn(open.update, system));
    expect(back.undone![0]).toEqual(steps[0]);
    expect(back.undone!.at(-1)).toEqual({ kind: "done_moving" });
    expect(steps).toEqual(expect.arrayContaining(back.undone!));
    // The activation is the seat's own too, and nothing was shown since: it can go back.
    expect(back.canUndo).toBe(true);
    game.transport.close();
  }, 60_000);

  it("stops a plan where the game asks for something else, with its steps in the game", async () => {
    const game = await start();
    const { system, open, facts } = await toMovement(game);
    // A ship moves with one infantry in its hold, so the game asks for the landings next.
    const ship = facts.ships.find((item) => canMove(facts, {}, item) && item.loads?.length)!;
    const pool = ship.loads!.find((at) => facts.cargo[at].unit.includes("infantry"))!;
    const moving = setLoad(facts, setShip(facts, {}, shipId(ship), true), shipId(ship), pool, 1);
    const steps: PlanStep[] = stepsOf(facts, moving);
    // The plan has no landing: its next step is one that the game does not ask for there.
    const rest: PlanStep[] = [{ kind: "produce", unit: "fighter" }];
    game.transport.submitPlan(nonce(open), { destination: system, steps: [...steps, ...rest] });
    const stopped = await game.until((event) => !!event.plan);
    expect(stopped.plan).toMatchObject({
      destination: system,
      applied: steps.length,
      remaining: rest,
      interrupted: true,
    });
    // The movement is in the game, and the question that stopped the plan is open.
    expect(subtype(stopped)).toBe("commit_ground_forces");
    expect(shipsIn(stopped.update, system)).toContain(ship.unit);
    game.transport.close();
  }, 60_000);

  it("names the strategic action, plays the draft of a secondary, and sends it", async () => {
    const game = await start();
    // The seat plays on until another seat has played a strategy card and the game asks it
    // for the secondary. (The steps of a local game show the action earlier: the primary.)
    let answered: string | undefined;
    let event = await asked(game);
    for (let answers = 0; choiceOf(event)?.details?.kind !== "strategy_secondary"; answers += 1) {
      expect(answers).toBeLessThan(300);
      const { options } = choiceOf(event)!;
      const option = options.find((item) => item.kind === "decline") ?? options[0];
      answered = nonce(event);
      game.transport.submitChoice(answered, option.id);
      event = await asked(game, answered);
    }
    const question = choiceOf(event)!;
    const me = seat(event.update);
    const action = event.update.secondary!;
    expect(action).toMatchObject({
      card: question.details?.card,
      primary: question.details?.played_by,
      open: true,
      draft: true,
    });
    expect(action.primary).not.toBe(me);
    expect(action.followers.find((follower) => follower.seat === me)?.state).toBe("waiting");

    // The draft asks what the game asks, on a copy.
    const scope: PlanScope = { kind: "secondary", primary: action.primary, card: action.card };
    const open = await game.draft([], scope);
    expect(open?.stop).toEqual({ kind: "open" });
    expect(open?.update.pending_choice?.choice.options.map((option) => option.id)).toEqual(
      question.options.map((option) => option.id),
    );
    const pass: DraftStep = { kind: "choose", option_id: question.options[0].id };
    const passed = await game.draft([pass], scope);
    expect(passed?.stop).toEqual({ kind: "complete" });
    expect(passed?.consumed).toBe(1);
    // Only the secondary of the action that is resolved is drafted.
    const other = await game.draft([], { ...scope, primary: me });
    expect(other?.stop.kind).toBe("failed");

    // Nothing of it is in the game: the question is still open, and the draft answers it.
    game.transport.applyDraft(nonce(event), { choices: [pass.option_id], plan: null });
    // The window goes on without the seat. A local game says nothing until the seat is asked
    // again, which may be in another action.
    const mine = (update: SessionUpdate) =>
      update.secondary?.card === action.card && update.secondary.primary === action.primary
        ? update.secondary.followers.find((follower) => follower.seat === me)?.state
        : undefined;
    const after = await game.until((next) => mine(next.update) !== "waiting");
    expect(after.plan).toBeUndefined();
    expect(mine(after.update) ?? "passed").toBe("passed");
    game.transport.close();
  }, 60_000);

  it("plays a draft on a copy, and sends it as one request", async () => {
    const game = await start();
    const menu = await playTo(game, "action_menu");
    const tactical: DraftStep[] = [{ kind: "choose", option_id: "tactical" }];
    const activation = await game.draft(tactical);
    expect(activation?.stop).toEqual({ kind: "open" });
    expect(activation?.consumed).toBe(1);
    const system = target(activation!.update);
    const activated: DraftStep[] = [...tactical, { kind: "choose", option_id: system }];
    const movement = await game.draft(activated);
    expect(movement?.stop).toEqual({ kind: "open" });
    const facts = movement?.update.tactical as MovementFacts;
    expect(facts).toMatchObject({ kind: "movement", active: system, moved: 0 });
    // The update of a draft is of the copy, with all the facts of an update.
    expect(movement?.update).toHaveProperty("score");
    const { ship, steps } = oneShip(facts);
    const moved = await game.draft([...activated, ...steps]);
    expect(moved?.stop.kind).not.toBe("refused");
    expect(moved?.stop.kind).not.toBe("failed");
    expect(moved?.consumed).toBe(activated.length + steps.length);
    // A step that the game does not offer ends the run there, and says which.
    const wrong = { ...(steps[0] as PlanStep & { kind: "move" }), unit: "no_such_ship" };
    expect((await game.draft([...activated, wrong]))?.stop).toMatchObject({
      kind: "refused",
      step: 2,
    });
    // Nothing of it is in the game: the menu is still the open choice, and the draft is sent to it.
    expect(shipsIn(menu.update, system)).not.toContain(ship.unit);
    game.transport.applyDraft(nonce(menu), {
      choices: ["tactical", system],
      plan: { destination: system, steps },
    });
    const after = await game.until((event) => shipsIn(event.update, system).includes(ship.unit));
    expect(after.plan).toBeUndefined();
    expect(after.update.view.board.systems[system].command_tokens).toContain(seat(menu.update));
    game.transport.close();
  }, 60_000);

  it("keeps the cards that are never offered, and tells each change", async () => {
    const game = await start();
    await playTo(game, "action_menu");
    expect(game.never()).toEqual([]);
    game.transport.setReactionMode("Sabotage", "never");
    await game.cards(["Sabotage"]);
    game.transport.setReactionMode("Sabotage", "never");
    game.transport.setReactionMode("Sabotage", "always");
    await game.cards([]);
    // The card of an ability is set by its key, and "only on my turns" is a mode of its own.
    game.transport.setReactionMode("leader:solagent", "ownturn");
    await game.mode("leader:solagent", "ownturn");
    expect(game.never()).toEqual([]);
    game.transport.setReactionMode("leader:solagent", "always");
    await game.mode("leader:solagent", "always");
    expect(game.modes()).toEqual({});
    game.transport.close();
  }, 60_000);

  it("computes the odds of a space combat in the page, the same for the same sides", async () => {
    const game = await start();
    const sides = {
      attacker: { faction: "sol", units: { dreadnought: 2, carrier: 1, fighter: 3 } },
      defender: { faction: "letnev", units: { cruiser: 2, destroyer: 1 }, guns: { pds: 1 } },
    };
    const odds = await game.transport.odds(sides);
    expect(odds.attacker_fielded).toEqual({ dreadnought: 2, carrier: 1, fighter: 3 });
    expect(odds.attacker_win_rate).toBeGreaterThan(odds.defender_win_rate);
    expect(
      odds.attacker_win_rate +
        odds.defender_win_rate +
        odds.mutual_destruction_rate +
        odds.unresolved_rate,
    ).toBeCloseTo(1);
    expect(await game.transport.odds(sides)).toEqual(odds);
    // Sides that the engine does not read are refused, and the next question is answered.
    await expect(
      game.transport.odds({ ...sides, attacker: { faction: "sol", units: { nothing: 1 } } }),
    ).rejects.toThrow();
    expect(await game.transport.odds(sides)).toEqual(odds);
    game.transport.close();
  }, 60_000);
}

describe.skipIf(!existsSync(WASM))("the engine in this page", () => {
  let engine: Engine;
  beforeAll(async () => {
    const module = await WebAssembly.compile(readFileSync(WASM));
    engine = {
      module,
      hash: "test",
      scenarios: [],
      catalog: await loadCatalog({ query: engineQuery(module) }),
    };
  });
  contract(async () => side(createWasmTransport(engine, { seed: 3, players: 8, humans: 1 })));
});

describe.skipIf(!existsSync(WASM) || !SERVER)("a server", () => {
  let server: TestServer;
  let engine: Pick<Engine, "catalog" | "module">;
  beforeAll(async () => {
    // The printed cards are those of the engine of this page, as in the browser: the transport
    // plays only when the server has the same corpus. The odds are of that engine too.
    const module = await WebAssembly.compile(readFileSync(WASM));
    engine = { catalog: await loadCatalog({ query: engineQuery(module) }), module };
    // A seat that the server plays answers at once: by default it waits before each answer.
    server = await spawnTestServer({ env: { TI4_RANDOM_BOT_DELAY_MS: "0" } });
  }, 60_000);
  afterAll(() => server?.stop());
  /** A game of eight with one seat for this side, and the transport of that seat. */
  const play = async (made?: { game_id: string; player_session: string }) => {
    if (!made) {
      const response = await fetch(`${server.url}/api/games`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          player_count: 8,
          seed: 3,
          nickname: "Contract",
          fill_seats: "random",
        }),
      });
      expect(response.status).toBe(200);
      made = await response.json();
    }
    const game = made!;
    return {
      made: game,
      transport: createWsTransport(
        engine,
        { gameId: game.game_id, playerSession: game.player_session },
        { open: (id) => openSocket(`ws://127.0.0.1:${server.port}/ws/games/${id}`) },
      ),
    };
  };
  // Only the server keeps what a seat staged: a game in this page is in one tab.
  it("keeps what a seat staged for every connection of the seat", async () => {
    const first = await play();
    const second = await play(first.made);
    const a = first.transport.staged!;
    const b = second.transport.staged!;
    /** The next value of a slot that the other connection wrote. */
    const next = (store: typeof b) =>
      new Promise<unknown>((told) => {
        const stop = store.subscribe((slot, value) => {
          stop();
          told([slot, value]);
        });
      });
    // The second tab is told what the first staged, as the server keeps it.
    const draft = { history: [{ system: "26", steps: null }], cursor: 0 };
    let told = next(b);
    a.set("plan", draft);
    expect(await told).toEqual(["plan", draft]);
    expect(b.get("plan")).toEqual(draft);
    // And the first what the second clears.
    told = next(a);
    b.set("plan", null);
    expect(await told).toEqual(["plan", null]);
    // A tab that comes later has it at once.
    a.set("secondary", { card: "pok3politics", answers: [], ready: false });
    await new Promise((settle) => setTimeout(settle, 500));
    const late = await play(first.made);
    const told3 = await next(late.transport.staged!);
    expect(told3).toEqual(["secondary", { answers: [], card: "pok3politics", ready: false }]);
    for (const side of [first, second, late]) {
      side.transport.close();
    }
  }, 30_000);
  contract(async () => {
    const response = await fetch(`${server.url}/api/games`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        player_count: 8,
        seed: 3,
        nickname: "Contract",
        fill_seats: "random",
      }),
    });
    expect(response.status).toBe(200);
    const made = await response.json();
    return side(
      createWsTransport(
        engine,
        { gameId: made.game_id, playerSession: made.player_session },
        { open: (id) => openSocket(`ws://127.0.0.1:${server.port}/ws/games/${id}`) },
      ),
    );
  });
});
