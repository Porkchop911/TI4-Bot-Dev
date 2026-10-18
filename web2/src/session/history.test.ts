import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it, vi } from "vitest";
import type { EventLog } from "./events";
import {
  type BoardCheck,
  atStepEnd,
  boardAt,
  boardDifference,
  boardForward,
  checkBoard,
  publicBoardOf,
} from "./history";
import type { TransportEvent } from "./transport";
import { type Engine, createWasmTransport } from "./wasmTransport";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/**
 * A game of a scenario, seat a played here with the first option of each of `answers`
 * questions: every update of it, up to the last at the end of a step.
 */
async function game(scenario: string, answers = 40): Promise<Update[]> {
  const engine: Engine = {
    module: await WebAssembly.compile(readFileSync(WASM)),
    hash: "test",
    scenarios: [],
    catalog: null,
  };
  const transport = createWasmTransport(engine, { seed: 0, players: 3, humans: 1, scenario }, {});
  const seen: Update[] = [];
  let wake: (() => void) | null = null;
  transport.subscribe((event) => {
    if (event.kind === "error") {
      throw new Error(event.message);
    }
    if (event.kind === "update") {
      seen.push(event);
      wake?.();
    }
  });
  let read = 0;
  for (let answer = 0; answer < answers; answer += 1) {
    while (seen.length <= read) {
      await new Promise<void>((resume) => (wake = resume));
    }
    const pending = seen.at(-1)!.update.pending_choice;
    read = seen.length;
    if (pending) {
      transport.submitChoice(pending.nonce, pending.choice.options[0].id);
    }
  }
  // The game as the last step left it: an update inside a step comes before its change.
  return seen.slice(0, seen.findLastIndex((update) => atStepEnd(update.events ?? [])) + 1);
}

const changesOf = (events: EventLog) =>
  events.flatMap((entry) =>
    entry.event.kind === "board_changed" && entry.board ? [entry.board] : [],
  );

/**
 * Each update at the end of a step is the board of now rewound to its events, and forward again
 * to the last one.
 */
function checkRewind(updates: Update[]) {
  const last = updates.at(-1)!;
  const events = last.events ?? [];
  const end = publicBoardOf(last.update.view);
  let checked = 0;
  for (const update of updates) {
    const told = update.events ?? [];
    // The events of an earlier update are the start of the events of the last one.
    expect(told.map((entry) => entry.id)).toEqual(
      events.slice(0, told.length).map((entry) => entry.id),
    );
    if (!atStepEnd(told)) {
      continue;
    }
    checked += 1;
    const then = publicBoardOf(update.update.view);
    expect(boardDifference(boardAt(last.update.view, events, told.length), then)).toEqual([]);
    expect(boardDifference(boardForward(then, events, told.length), end)).toEqual([]);
  }
  expect(checked).toBeGreaterThan(3);
  // A change that is applied twice changes nothing: its values are whole.
  const first = boardAt(last.update.view, events, 0);
  const twice = events.flatMap((entry) => [entry, entry]);
  expect(boardDifference(boardForward(first, twice, 0), end)).toEqual([]);
}

it.skipIf(!built)(
  "the changes of the board take a space combat back to every update, with its rounds and dice",
  async () => {
    const updates = await game("ongoing_combat");
    const last = updates.at(-1)!;
    const events = last.events ?? [];
    const changes = changesOf(events);
    expect(changes.some((delta) => delta.space_rounds?.after.length)).toBe(true);
    expect(changes.some((delta) => delta.combat)).toBe(true);
    checkRewind(updates);
    // A board of the combat under way has the dice that the update had.
    const fought = updates.filter((update) => update.update.view.board.combat?.dice_rolls?.length);
    expect(fought.length).toBeGreaterThan(0);
    for (const update of fought) {
      const then = boardAt(last.update.view, events, update.events!.length);
      expect(then.board.combat?.dice_rolls).toEqual(update.update.view.board.combat?.dice_rolls);
    }
    // Each round of the turn has its dice.
    const rounds = changes.flatMap((delta) => delta.space_rounds?.after ?? []);
    expect(rounds.every((round) => round.dice_rolls.length > 0)).toBe(true);
  },
  60_000,
);

it.skipIf(!built)(
  "the changes of the board take an invasion back to every update, with its ground rounds",
  async () => {
    const updates = await game("ongoing_invasion_four_views");
    const changes = changesOf(updates.at(-1)!.events ?? []);
    expect(changes.some((delta) => delta.ground_steps?.after.length)).toBe(true);
    expect(changes.some((delta) => delta.invasion)).toBe(true);
    checkRewind(updates);
  },
  60_000,
);

it.skipIf(!built)(
  "the check of development is quiet on a game, and says when a change of the board is missing",
  async () => {
    const updates = await game("ongoing_combat");
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    let check: BoardCheck = null;
    for (const update of updates) {
      check = checkBoard(check, update.update.view, update.events ?? []);
    }
    expect(error).not.toHaveBeenCalled();

    // The same game with a change of the board that says one unit too many: the last change of
    // units, so that no later change of the same spot puts its count right again.
    const events = updates.at(-1)!.events ?? [];
    const wrong = events.findLastIndex((entry) => entry.board?.units?.length);
    expect(wrong).toBeGreaterThan(0);
    const told = events.map((entry, at) => {
      if (at !== wrong) {
        return entry;
      }
      const [first, ...rest] = entry.board!.units!;
      return {
        ...entry,
        board: { ...entry.board, units: [{ ...first, after: first.after + 1 }, ...rest] },
      };
    });
    check = null;
    for (const update of updates) {
      check = checkBoard(check, update.update.view, told.slice(0, update.events?.length ?? 0));
    }
    expect(error).toHaveBeenCalledWith(expect.stringContaining("[board history]"));
    error.mockRestore();
  },
  60_000,
);
