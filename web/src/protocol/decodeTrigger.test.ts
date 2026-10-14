import { describe, expect, it } from "vitest";
import { decodeDecisionTrigger } from "./decode.ts";

describe("decodeDecisionTrigger", () => {
  it("keeps a well-formed trigger as the engine sends it", () => {
    expect(
      decodeDecisionTrigger({
        kind: "ship_moved",
        event_type: "SHIP_MOVED",
        event_id: 9,
        relation: "after",
        actor: "p1",
        system: "27",
        units: [{ owner: "p1", unit_type: "cruiser", count: 2 }],
        chain: [3],
      }),
    ).toEqual({
      kind: "ship_moved",
      event_type: "SHIP_MOVED",
      event_id: 9,
      relation: "after",
      actor: "p1",
      system: "27",
      units: [{ owner: "p1", unit_type: "cruiser", count: 2 }],
      chain: [3],
    });
  });

  it("returns null for absent or damaged data so old saves and fixtures still load", () => {
    expect(decodeDecisionTrigger(undefined)).toBeNull();
    expect(decodeDecisionTrigger(null)).toBeNull();
    expect(decodeDecisionTrigger("x")).toBeNull();
    expect(decodeDecisionTrigger({ kind: "ship_moved" })).toBeNull();
  });

  it("reads an unknown kind as other and drops malformed units", () => {
    const decoded = decodeDecisionTrigger({
      event_type: "NEW_EVENT",
      relation: "when",
      units: [{ owner: 1 }, { owner: "p1", unit_type: "mech", count: 1 }],
    });
    expect(decoded?.kind).toBe("other");
    expect(decoded?.units).toEqual([{ owner: "p1", unit_type: "mech", count: 1 }]);
  });
});
