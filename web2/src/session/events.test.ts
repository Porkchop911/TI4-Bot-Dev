import { describe, expect, it } from "vitest";
import { NO_EVENTS, eventsOf, withEvent } from "./events";
import type { GameEvent } from "./wire";

const event = (id: number, change: Partial<GameEvent> = {}): GameEvent => ({
  id: `g-${id}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "decision_resolved" },
  ...change,
});

describe("the public events of the game", () => {
  it("keeps each public event once, in the order of the game", () => {
    const log = eventsOf([event(1), event(2)]);
    const more = withEvent(log, event(3));
    expect(more.map((entry) => entry.id)).toEqual(["g-1", "g-2", "g-3"]);
    // A reconnection tells an event again: the log stays as it was.
    expect(withEvent(more, event(2))).toBe(more);
    expect(withEvent(more, event(3))).toBe(more);
  });

  it("keeps no event of a seat or of the referee", () => {
    const seat = event(1, { visibility: { visibility: "seat", seat: "a" } });
    const referee = event(2, { visibility: { visibility: "referee" } });
    expect(eventsOf([seat, referee, event(3)]).map((entry) => entry.id)).toEqual(["g-3"]);
    expect(withEvent(NO_EVENTS, seat)).toBe(NO_EVENTS);
  });

  it("is the events of a snapshot alone: an undo took the later ones back", () => {
    const before = withEvent(eventsOf([event(1), event(2)]), event(3));
    const after = eventsOf([event(1), event(2)]);
    expect(after.map((entry) => entry.id)).toEqual(["g-1", "g-2"]);
    expect(before).toHaveLength(3);
    expect(eventsOf()).toEqual([]);
  });
});
