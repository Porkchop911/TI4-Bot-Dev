import { describe, it, expect, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";
import type { GameEvent } from "../protocol/types.ts";
import { useCornerToasts } from "./useCornerToasts.ts";
import { TURN_RECAP_KEY } from "./useTurnRecapSetting.ts";
import { TOAST_MUTE_KEY } from "./useToastMute.ts";

let n = 0;
function ev(actionId: string, actionActor: string, actor: string, what?: string, extra: Partial<GameEvent> = {}): GameEvent {
  n += 1;
  return {
    id: `h${n}`,
    timestamp: "1",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    action_id: actionId,
    action_actor: actionActor,
    actor,
    detail: what ? `${actor} ${what}` : undefined,
    ...extra,
  } as GameEvent;
}
const select = (id: string, actor: string, type: string) =>
  ev(id, actor, actor, undefined, { stage: "action selection", action_type: type });

type Props = Parameters<typeof useCornerToasts>[0];
const base: Props = { events: [], viewerSeat: "p1", ready: true };
const recaps = (r: { current: { notifications: { kind?: string }[] } }) =>
  r.current.notifications.filter((t) => t.kind === "recap");

function setup(initial: GameEvent[] = []) {
  const hook = renderHook((p: Props) => useCornerToasts(p), { initialProps: { ...base, events: initial } });
  return hook;
}

describe("turn recap toast", () => {
  beforeEach(() => localStorage.clear());

  const turnOne = [
    select("t1", "p2", "tactical"),
    ev("t1", "p2", "p2", "activated #27", { stage: "activation" }),
    ev("t1", "p2", "p2", "moved cruiser from #1 to #27", { stage: "movement" }),
  ];

  it("shows nothing by default: other players' steps toast live as before", () => {
    const { result, rerender } = setup();
    rerender({ ...base, events: [...turnOne, select("t2", "p3", "pass")] });
    expect(recaps(result)).toHaveLength(0);
    expect(result.current.notifications.length).toBeGreaterThan(0);
  });

  it("shows one recap when the turn ends and replaces that player's live toasts", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    const { result, rerender } = setup();
    rerender({ ...base, events: turnOne });
    // Mid-turn: nothing yet, and no live toast for the acting player's own steps.
    expect(result.current.notifications).toHaveLength(0);
    const next = [...turnOne, select("t2", "p3", "tactical")];
    rerender({ ...base, events: next });
    expect(result.current.notifications).toHaveLength(1);
    expect(result.current.notifications[0]).toMatchObject({
      kind: "recap",
      actor: "p2",
      text: "tactical action in system 27: moved 1 ship",
    });
  });

  it("recaps a pass immediately, and never the viewer's own turn", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    const { result, rerender } = setup();
    rerender({ ...base, events: [select("m1", "p1", "pass")] });
    expect(recaps(result)).toHaveLength(0);
    rerender({ ...base, events: [select("m1", "p1", "pass"), select("m2", "p2", "pass")] });
    expect(recaps(result)).toMatchObject([{ actor: "p2", text: "passed" }]);
  });

  it("closes the turn when the viewer is asked for their own action menu", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    const { result, rerender } = setup();
    rerender({ ...base, events: turnOne });
    expect(recaps(result)).toHaveLength(0);
    rerender({
      ...base,
      events: turnOne,
      pendingChoice: { prompt: "action phase", actor: "p1", nonce: "z", options: [] },
    });
    expect(recaps(result)).toHaveLength(1);
  });

  it("is silenced by the toast mute", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    localStorage.setItem(TOAST_MUTE_KEY, "true");
    const { result, rerender } = setup();
    rerender({ ...base, events: [select("q1", "p2", "pass")] });
    expect(result.current.notifications).toHaveLength(0);
  });

  it("does not fire for history present at load, a replay of known entries, or a mid-turn load", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    const history = [...turnOne, select("t2", "p3", "pass")];
    const { result, rerender } = setup(history);
    expect(result.current.notifications).toHaveLength(0);
    // Same ids delivered again (a reconnect's new snapshot): nothing new.
    rerender({ ...base, events: history.map((e) => ({ ...e })) });
    expect(result.current.notifications).toHaveLength(0);
    // The turn that was half done at load never gets a recap; the next one does.
    const half = select("l1", "p2", "tactical");
    const mid = setup([half]);
    mid.rerender({ ...base, events: [half, ev("l1", "p2", "p2", "activated #3"), select("l2", "p3", "pass")] });
    expect(recaps(mid.result)).toMatchObject([{ actor: "p3" }]);
  });

  it("shows only the latest turn when a reconnect delivers several at once", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    const { result, rerender } = setup([select("c0", "p2", "pass")]);
    rerender({
      ...base,
      events: [select("c0", "p2", "pass"), select("c1", "p3", "pass"), select("c2", "p4", "pass"), select("c3", "p2", "pass")],
    });
    expect(recaps(result)).toHaveLength(1);
  });

  it("forgets an open turn that an undo removed or a new history generation replaced", () => {
    localStorage.setItem(TURN_RECAP_KEY, "true");
    const u2 = select("u2", "p3", "tactical");
    const u3 = select("u3", "p2", "tactical");
    const { result, rerender } = renderHook((p: Props) => useCornerToasts(p), {
      initialProps: { ...base, historyGeneration: 0 } as Props,
    });
    rerender({ ...base, historyGeneration: 0, events: turnOne });
    // Undo takes the turn away; another player then starts a turn: no recap of the undone one.
    rerender({ ...base, historyGeneration: 0, events: [] });
    rerender({ ...base, historyGeneration: 0, events: [u2] });
    expect(recaps(result)).toHaveLength(0);
    // A new generation replaces the timeline: the open turn is dropped without a recap.
    rerender({ ...base, historyGeneration: 1, events: [u2] });
    rerender({ ...base, historyGeneration: 1, events: [u2, u3] });
    expect(recaps(result)).toHaveLength(0);
    // Redo of the old entries (already seen) does not toast either.
    rerender({ ...base, historyGeneration: 1, events: turnOne });
    expect(recaps(result)).toHaveLength(0);
  });
});
