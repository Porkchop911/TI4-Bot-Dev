// The public events of the game (`GameEvent`), as the transport is told them: kept for the Log
// (`select/log.ts`), which reads the turns of the seats from them. `rolls.ts` keeps the dice of
// the action that is played now from the same events.
import type { GameEvent } from "./wire";

/** The public events of the game in the order of the game, each once. */
export type EventLog = readonly GameEvent[];

export const NO_EVENTS: EventLog = [];

const isPublic = (entry: GameEvent) => entry.visibility.visibility === "public";

/**
 * The log with one more event. An event that is not public is not kept, and an event is kept
 * once: a restart of the game, or a reconnection, tells it again with the same `id`.
 */
export function withEvent(log: EventLog, entry: GameEvent): EventLog {
  if (!isPublic(entry)) {
    return log;
  }
  // A repeat is of the latest events: the search starts at the end.
  for (let at = log.length - 1; at >= 0; at -= 1) {
    if (log[at].id === entry.id) {
      return log;
    }
  }
  return [...log, entry];
}

/**
 * The log of a snapshot: every public event that it has. It replaces the log before it, since an
 * undo takes the later events back.
 */
export const eventsOf = (events: readonly GameEvent[] = []): EventLog =>
  events.reduce(withEvent, NO_EVENTS);
