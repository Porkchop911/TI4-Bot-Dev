import { describe, expect, it } from "vitest";
import { GameLogEntry, serverEventLog } from "./useGameSession.ts";

const entry = (id: string): GameLogEntry => ({
  id,
  timestamp: "12:00:00",
  version: 1,
  visibility: "public",
  event: { kind: "decision_resolved" },
});

describe("serverEventLog", () => {
  it("renders only server-authored entries without synthesizing an initialization record", () => {
    expect(serverEventLog(undefined)).toEqual([]);
    expect(serverEventLog([entry("server-1")])).toEqual([entry("server-1")]);
  });

  it("keeps the complete authoritative event stream", () => {
    const events = Array.from({ length: 501 }, (_, index) => entry(`server-${index}`));
    const bounded = serverEventLog(events);

    expect(bounded).toHaveLength(501);
    expect(bounded[0]?.id).toBe("server-0");
    expect(bounded.at(-1)?.id).toBe("server-500");
  });

  it("preserves a batch and same-cursor phase event beyond 500 entries", () => {
    const events = Array.from({ length: 504 }, (_, index): GameLogEntry => ({
      ...entry(`server-${index}`),
      decision_count: index,
      ...(index >= 3 && index <= 7
        ? {
            batch_id: "batch",
            batch_start_cursor: 2,
            batch_end_cursor: 8,
          }
        : {}),
    }));
    events[4] = { ...events[4], event: { kind: "phase_transition", phase: "action", round: 1 } };
    const bounded = serverEventLog(events);
    expect(bounded[0].id).toBe("server-0");
    expect(bounded.filter((event) => event.batch_id === "batch")).toHaveLength(5);
  });
});
