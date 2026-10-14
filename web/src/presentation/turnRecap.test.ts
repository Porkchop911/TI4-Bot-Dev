import { describe, it, expect } from "vitest";
import type { GameEvent } from "../protocol/types.ts";
import { buildTurnRecap, trackTurns, type OpenTurn } from "./turnRecap.ts";

let n = 0;
/** A public decision of the action phase, as the server logs it. */
function ev(
  actionId: string,
  actionActor: string,
  actor: string,
  what: string | undefined,
  extra: Partial<GameEvent> = {},
): GameEvent {
  n += 1;
  return {
    id: `e${n}`,
    timestamp: "1",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    action_id: actionId,
    action_actor: actionActor,
    actor,
    detail: what === undefined ? undefined : `${actor} ${what}`,
    ...extra,
  } as GameEvent;
}
const select = (id: string, actor: string, type: string) =>
  ev(id, actor, actor, undefined, { stage: "action selection", action_type: type });

function turn(entries: GameEvent[]): OpenTurn {
  const first = entries[0];
  return { actionId: first.action_id!, actor: first.action_actor!, entries };
}

describe("buildTurnRecap", () => {
  it("sums up a tactical action with movement, combat and production", () => {
    const recap = buildTurnRecap(
      turn([
        select("a1", "p2", "tactical"),
        ev("a1", "p2", "p2", "activated #27", { stage: "activation" }),
        ev("a1", "p2", "p2", "moved cruiser from #1 to #27", { stage: "movement" }),
        ev("a1", "p2", "p2", "moved cruiser from #1 to #27", { stage: "movement" }),
        ev("a1", "p2", "p2", "moved carrier from #3 to #27", { stage: "movement" }),
        ev("a1", "p2", "p2", "lost a fighter in #27", { stage: "combat" }),
        ev("a1", "p2", "p3", "lost a cruiser in #27", { stage: "combat" }),
        ev("a1", "p2", "p3", "lost a destroyer in #27", { stage: "combat" }),
        ev("a1", "p2", "p2", "produced 2 fighter", { stage: "production" }),
      ]),
    );
    expect(recap).toEqual({
      id: "recap:a1",
      actor: "p2",
      text: "tactical action in system 27: moved 3 ships, fought (lost 1, destroyed 2), built 2 units",
    });
  });

  it("sums up a strategic action by what the log shows", () => {
    const recap = buildTurnRecap(
      turn([
        select("a2", "p2", "strategic"),
        ev("a2", "p2", "p2", "gained a tactic command token", { stage: "strategy" }),
        ev("a2", "p2", "p2", "gained a fleet command token", { stage: "strategy" }),
        ev("a2", "p2", "p2", "researched Sarween Tools", { stage: "strategy" }),
        // Another seat's secondary step is not the actor's own doing.
        ev("a2", "p2", "p3", "researched Gravity Drive", { stage: "strategy" }),
      ]),
    );
    expect(recap?.text).toBe("strategic action: gained 2 command tokens, researched Sarween Tools");
  });

  it("says passed for a pass", () => {
    expect(buildTurnRecap(turn([select("a3", "p4", "pass")]))).toEqual({
      id: "recap:a3",
      actor: "p4",
      text: "passed",
    });
  });

  it("states only the action for a turn with nothing public in it (no-op)", () => {
    expect(buildTurnRecap(turn([select("a4", "p2", "component")]))?.text).toBe("component action");
    expect(
      buildTurnRecap(turn([select("a4", "p2", "tactical"), ev("a4", "p2", "p2", "activated #9")]))?.text,
    ).toBe("tactical action in system 9");
  });

  it("uses nothing that is redacted or not public", () => {
    const secret = ev("a5", "p2", "p2", "played Sabotage", { visibility: { visibility: "seat", seat: "p2" } } as never);
    const referee = ev("a5", "p2", "p2", "scored Secret Hold", { visibility: { visibility: "referee" } } as never);
    const blank = ev("a5", "p2", "p2", undefined, { stage: "production" });
    const recap = buildTurnRecap(turn([select("a5", "p2", "tactical"), secret, referee, blank]));
    expect(recap?.text).toBe("tactical action");
  });

  it("returns null without a selection entry and trims long lists", () => {
    expect(buildTurnRecap(turn([ev("a6", "p2", "p2", "activated #1")]))).toBeNull();
    const long = buildTurnRecap(
      turn([
        select("a7", "p2", "component"),
        ev("a7", "p2", "p2", "played A"),
        ev("a7", "p2", "p2", "played B"),
        ev("a7", "p2", "p2", "researched C"),
        ev("a7", "p2", "p2", "scored D"),
        ev("a7", "p2", "p2", "played E"),
      ]),
    );
    expect(long?.text).toBe("component action: played A, played B, researched C, scored D, and 1 more");
  });
});

describe("trackTurns", () => {
  it("closes a turn when the next action starts", () => {
    const first = trackTurns(null, [select("x1", "p2", "tactical"), ev("x1", "p2", "p2", "activated #5")], "p1");
    expect(first.closed).toHaveLength(0);
    expect(first.open?.actionId).toBe("x1");
    const second = trackTurns(first.open, [select("x2", "p3", "strategic")], "p1");
    expect(second.closed.map((t) => t.actionId)).toEqual(["x1"]);
    expect(second.open?.actor).toBe("p3");
  });

  it("closes a pass at once, and on a phase change", () => {
    expect(trackTurns(null, [select("y1", "p2", "pass")], "p1")).toMatchObject({ open: null, closed: [{ actor: "p2" }] });
    const open = trackTurns(null, [select("y2", "p2", "tactical")], "p1").open;
    const phase = { id: "ph", timestamp: "1", visibility: { visibility: "public" as const }, event: { kind: "phase_transition", phase: "status", round: 2 } } as GameEvent;
    expect(trackTurns(open, [phase], "p1").closed).toHaveLength(1);
  });

  it("never opens the viewer's own turn or a turn it saw only the middle of", () => {
    expect(trackTurns(null, [select("z1", "p1", "tactical")], "p1").open).toBeNull();
    expect(trackTurns(null, [ev("z2", "p2", "p2", "moved cruiser from #1 to #2")], "p1").open).toBeNull();
  });
});
