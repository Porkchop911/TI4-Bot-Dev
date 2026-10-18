// The public board at a point of the events: the board of now taken back by the changes of the
// board that the events told (`board_changed`, `BoardDelta` of `ti4-host/src/protocol/
// board_delta.rs`). Each part of a change is a whole value before and after the step (a count of
// units at a spot, a planet, the command tokens of a system, the pools of a seat, the combat under
// way), so a change put back twice changes nothing. Only what every seat sees is in it: the board
// view with no map tiles, and the pools of each seat.
import type { EventLog } from "./events";
import type {
  BoardDelta,
  BoardView,
  GameEvent,
  GameView,
  PlacedUnitView,
  PlayerId,
  SeatPools,
} from "./wire";

/** The public board: the board view, and the pools of each seat. */
export type PublicBoard = {
  board: BoardView;
  pools: Record<PlayerId, SeatPools>;
};

const poolsOf = (view: GameView): Record<PlayerId, SeatPools> =>
  Object.fromEntries(
    view.players.map((player) => [
      player.id,
      {
        trade_goods: player.trade_goods,
        commodities: player.commodities,
        tactic_tokens: player.tactic_tokens,
        fleet_tokens: player.fleet_tokens,
        strategic_tokens: player.strategic_tokens,
      },
    ]),
  );

/** The public board of `view`, a copy that a change may write. */
export const publicBoardOf = (view: GameView): PublicBoard => ({
  board: structuredClone(view.board),
  pools: poolsOf(view),
});

const sameSpot = (
  unit: PlacedUnitView,
  change: NonNullable<BoardDelta["units"]>[number],
): boolean =>
  unit.owner === change.owner &&
  unit.unit_type === change.unit_type &&
  unit.planet === change.planet &&
  unit.damaged === change.damaged &&
  unit.galvanized === change.galvanized;

const pick = <T>(forward: boolean, before: T, after: T): T => (forward ? after : before);

/** The first `kept` stay, then the tail of the side. */
const splice = <T>(list: T[] | undefined, kept: number, tail: readonly T[]): T[] => [
  ...(list ?? []).slice(0, kept),
  ...structuredClone(tail as T[]),
];

/** Puts the values of one side of `delta` into `into`. It writes `into`. */
function apply(into: PublicBoard, delta: BoardDelta, forward: boolean) {
  const { board } = into;
  for (const change of delta.systems ?? []) {
    if (pick(forward, change.before, change.after)) {
      board.systems[change.system] ??= {
        system_id: change.system,
        command_tokens: [],
        planets: {},
        units: [],
      };
    } else {
      delete board.systems[change.system];
    }
  }
  for (const change of delta.units ?? []) {
    const system = board.systems[change.system];
    if (!system) {
      continue;
    }
    const count = pick(forward, change.before, change.after);
    const units = system.units.filter((unit) => !sameSpot(unit, change));
    for (let at = 0; at < count; at += 1) {
      units.push({
        unit_type: change.unit_type,
        owner: change.owner,
        planet: change.planet,
        damaged: change.damaged,
        galvanized: change.galvanized,
      });
    }
    system.units = units;
  }
  for (const change of delta.planets ?? []) {
    const system = board.systems[change.system];
    if (!system) {
      continue;
    }
    const planet = pick(forward, change.before, change.after);
    if (planet) {
      system.planets[change.planet] = structuredClone(planet);
    } else {
      delete system.planets[change.planet];
    }
  }
  for (const change of delta.tokens ?? []) {
    const system = board.systems[change.system];
    if (system) {
      system.command_tokens = [...pick(forward, change.before, change.after)];
    }
  }
  for (const change of delta.pools ?? []) {
    into.pools[change.seat] = { ...pick(forward, change.before, change.after) };
  }
  if (delta.active_system) {
    const active = pick(forward, delta.active_system.before, delta.active_system.after);
    if (active) {
      board.active_system = active;
    } else {
      delete board.active_system;
    }
  }
  if (delta.combat) {
    const combat = pick(forward, delta.combat.before, delta.combat.after);
    if (combat) {
      board.combat = structuredClone(combat);
    } else {
      delete board.combat;
    }
  }
  if (delta.invasion) {
    const invasion = pick(forward, delta.invasion.before, delta.invasion.after);
    if (invasion) {
      board.invasion = structuredClone(invasion);
    } else {
      delete board.invasion;
    }
  }
  if (delta.space_rounds) {
    const { kept, before, after } = delta.space_rounds;
    board.space_rounds = splice(board.space_rounds, kept, pick(forward, before, after));
  }
  if (delta.ground_steps) {
    const { kept, before, after } = delta.ground_steps;
    board.ground_steps = splice(board.ground_steps, kept, pick(forward, before, after));
  }
}

const changeOf = (entry: GameEvent): BoardDelta | undefined =>
  entry.event.kind === "board_changed" ? entry.board : undefined;

/** The board before the changes of `events[cursor..to]`, from the board after them. */
function rewound(from: PublicBoard, events: EventLog, cursor: number, to: number): PublicBoard {
  const board = structuredClone(from);
  for (let at = to - 1; at >= cursor; at -= 1) {
    const delta = changeOf(events[at]);
    if (delta) {
      apply(board, delta, false);
    }
  }
  return board;
}

/**
 * The board after the changes of `events[from..to]`, from `board`, the board before them. It
 * does not write `board`.
 */
export function boardForward(
  board: PublicBoard,
  events: EventLog,
  from: number,
  to = events.length,
): PublicBoard {
  const next = structuredClone(board);
  for (let at = from; at < to; at += 1) {
    const delta = changeOf(events[at]);
    if (delta) {
      apply(next, delta, true);
    }
  }
  return next;
}

/** The boards that were taken back from a view with its events, by their cursor. */
const taken = new WeakMap<GameView, { events: EventLog; at: Map<number, PublicBoard> }>();

/**
 * The public board after the first `cursor` events, from `view`, the game after all of `events`.
 * Kept for the view and its events: a later cursor that was taken back already is the start.
 * The board that it gives is shared: it must not be written.
 */
export function boardAt(view: GameView, events: EventLog, cursor: number): PublicBoard {
  let kept = taken.get(view);
  if (!kept || kept.events !== events) {
    kept = { events, at: new Map([[events.length, publicBoardOf(view)]]) };
    taken.set(view, kept);
  }
  const at = Math.max(0, Math.min(cursor, events.length));
  const known = kept.at.get(at);
  if (known) {
    return known;
  }
  // The nearest later point that is known.
  let start = events.length;
  for (const point of kept.at.keys()) {
    if (point > at && point < start) {
      start = point;
    }
  }
  const board = rewound(kept.at.get(start)!, events, at, start);
  kept.at.set(at, board);
  return board;
}

/**
 * Whether `events` end with the end of a step of the engine, where a board of the game is a board
 * of the changes: the change of the board is the last event of a step. A question that the engine
 * asks inside a step (a casualty in a round, a window) shows the game in the middle of it: the
 * decisions before it are told, the change of the board comes when the step ends.
 */
export const atStepEnd = (events: EventLog): boolean =>
  events.length === 0 || events[events.length - 1].event.kind === "board_changed";

/** `view` with the public board of `board`: the board view and the pools of each seat. */
export const withBoard = (view: GameView, { board, pools }: PublicBoard): GameView => ({
  ...view,
  board: { ...board, map_tiles: view.board.map_tiles },
  players: view.players.map((player) => ({ ...player, ...pools[player.id] })),
});

/** The units of a system in one order, for a comparison. */
const unitKey = (unit: PlacedUnitView) =>
  `${unit.owner}|${unit.unit_type}|${unit.planet ?? ""}|${unit.damaged}|${unit.galvanized}`;

/**
 * The combat as the board has it with no question: the seat asked, its step and the hits that it
 * assigns are of the question that a view of the seat has (`project_combat_view`).
 */
const combatOf = (combat: BoardView["combat"]) => {
  if (!combat) {
    return null;
  }
  const { active_player: _, stage: __, hits_to_assign: ___, ...board } = combat;
  return board;
};

/**
 * The public board in a form that compares as text: the order of units is not a part of it, nor
 * what a view has of its question.
 */
function normal({ board, pools }: PublicBoard) {
  const systems = Object.fromEntries(
    Object.entries(board.systems)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([id, system]) => [
        id,
        {
          command_tokens: [...system.command_tokens].sort(),
          planets: Object.fromEntries(
            Object.entries(system.planets).sort(([a], [b]) => a.localeCompare(b)),
          ),
          units: system.units.map(unitKey).sort(),
        },
      ]),
  );
  return {
    systems,
    active_system: board.active_system ?? null,
    combat: combatOf(board.combat),
    invasion: board.invasion ?? null,
    space_rounds: board.space_rounds ?? [],
    ground_steps: board.ground_steps ?? [],
    pools: Object.fromEntries(Object.entries(pools).sort(([a], [b]) => a.localeCompare(b))),
  };
}

/** Sorts the keys of objects, so that equal values are equal text. */
const sorted = (value: unknown): unknown =>
  Array.isArray(value)
    ? value.map(sorted)
    : value && typeof value === "object"
      ? Object.fromEntries(
          Object.entries(value)
            .filter(([, part]) => part !== undefined)
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([key, part]) => [key, sorted(part)]),
        )
      : value;

/**
 * Where two public boards differ: the names of the parts (a system, "combat", "pools"), empty
 * when they are the same board.
 */
export function boardDifference(a: PublicBoard, b: PublicBoard): string[] {
  const [left, right] = [normal(a), normal(b)];
  const differ: string[] = [];
  const ids = new Set([...Object.keys(left.systems), ...Object.keys(right.systems)]);
  for (const id of ids) {
    if (JSON.stringify(sorted(left.systems[id])) !== JSON.stringify(sorted(right.systems[id]))) {
      differ.push(`system ${id}`);
    }
  }
  for (const part of [
    "active_system",
    "combat",
    "invasion",
    "space_rounds",
    "ground_steps",
    "pools",
  ] as const) {
    if (JSON.stringify(sorted(left[part])) !== JSON.stringify(sorted(right[part]))) {
      differ.push(part);
    }
  }
  return differ;
}

/** The board of the last update at the end of a step, and the count of its events. */
export type BoardCheck = { board: PublicBoard; at: number } | null;

/**
 * A check of the changes for development: the board of the last update at the end of a step, put
 * forward by the changes since, is the board of `view` when it is at the end of a step too. A
 * difference is an error in the console; the check goes on from `view`. Only a game whose events
 * tell the changes of the board is checked.
 */
export function checkBoard(last: BoardCheck, view: GameView, events: EventLog): BoardCheck {
  if (events.length === 0 || !atStepEnd(events)) {
    return last;
  }
  const now = publicBoardOf(view);
  if (last && last.at <= events.length) {
    const differ = boardDifference(boardForward(last.board, events, last.at), now);
    if (differ.length > 0) {
      console.error(
        `[board history] the changes of events ${last.at} to ${events.length} do not reach the board of the game: ${differ.join(", ")}`,
      );
    }
  }
  return { board: now, at: events.length };
}
