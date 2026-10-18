// The public dice of the game, as its events tell them (`GameEvent.roll`): kept by the
// transport, so that a step can show what was rolled in the action that is played now.
import type { GameEvent, PublicRoll } from "./wire";

/** The rolls that the events told, and where the action that is played now starts. */
export interface RollLog {
  /**
   * The number of decisions before the action that is played now: its first decision is the
   * choice of the action. Null before the first event of an action.
   */
  start: number | null;
  /** Each roll once, with the number of decisions that were made before it. */
  rolls: { cursor: number; roll: PublicRoll }[];
}

export const NO_ROLLS: RollLog = { start: null, rolls: [] };

/**
 * The log with one more event. A roll is kept once: a restart of the game tells the same rolls
 * again, with the same `index`.
 */
export function withEvent(log: RollLog, entry: GameEvent): RollLog {
  const start =
    entry.action_start_cursor === undefined
      ? log.start
      : Math.max(log.start ?? 0, entry.action_start_cursor);
  const roll = entry.event.kind === "dice_rolled" ? entry.roll : undefined;
  if (!roll || log.rolls.some((known) => known.roll.index === roll.index)) {
    return start === log.start ? log : { ...log, start };
  }
  return { start, rolls: [...log.rolls, { cursor: entry.decision_count ?? 0, roll }] };
}

/** The log of a snapshot: every event that it has. */
export const logOf = (events: readonly GameEvent[] = []): RollLog =>
  events.reduce(withEvent, NO_ROLLS);

/** The rolls of the action that is played now, in the order of the game. */
export function actionRolls(log: RollLog): PublicRoll[] {
  const { start } = log;
  return start === null
    ? []
    : log.rolls
        .filter((item) => item.cursor > start)
        .map((item) => item.roll)
        .sort((a, b) => a.index - b.index);
}

/** The engine's tag of a roll without its detail ("gravity rift", "space cannon"). */
export const reasonOf = (roll: PublicRoll) => roll.reason.split(":")[0];
