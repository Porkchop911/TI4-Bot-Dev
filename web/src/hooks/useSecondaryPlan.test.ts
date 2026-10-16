import { describe, expect, it, beforeEach, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useSecondaryPrepMode, readSecondaryPrepMode, SECONDARY_PREP_MODE_KEY } from "./useSecondaryPrepMode.ts";
import { loadPlan, usePreparedPlan, PREPARED_KEY_PREFIX } from "./useSecondaryPlan.ts";

describe("useSecondaryPrepMode", () => {
  beforeEach(() => localStorage.clear());

  it("defaults to Review for everyone, with nothing stored", () => {
    expect(readSecondaryPrepMode()).toBe("review");
    expect(renderHook(() => useSecondaryPrepMode()).result.current.mode).toBe("review");
  });

  it("only an explicit stored auto turns auto on; anything else is review", () => {
    for (const value of ["", "true", "on", "AUTO", "review", "garbage"]) {
      localStorage.setItem(SECONDARY_PREP_MODE_KEY, value);
      expect(readSecondaryPrepMode()).toBe("review");
    }
    localStorage.setItem(SECONDARY_PREP_MODE_KEY, "auto");
    expect(readSecondaryPrepMode()).toBe("auto");
  });

  it("persists a change and keeps every user of the hook in sync", () => {
    const a = renderHook(() => useSecondaryPrepMode());
    const b = renderHook(() => useSecondaryPrepMode());
    act(() => a.result.current.setMode("auto"));
    expect(localStorage.getItem(SECONDARY_PREP_MODE_KEY)).toBe("auto");
    expect(b.result.current.mode).toBe("auto");
    act(() => b.result.current.setMode("review"));
    expect(a.result.current.mode).toBe("review");
  });

  it("falls back to review when storage throws", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readSecondaryPrepMode()).toBe("review");
    spy.mockRestore();
  });
});

describe("usePreparedPlan", () => {
  const plan = { card: "pok7technology", follow: true, tech: "amd" };
  interface Props {
    actionKey: string | null;
    generation: number;
    ready?: boolean;
    seat?: string | null;
    gameId?: string;
  }
  const setup = (initial: Props) =>
    renderHook(
      (p: Props) =>
        usePreparedPlan({
          gameId: p.gameId ?? "g1",
          viewerSeat: p.seat === undefined ? "b" : p.seat,
          actionKey: p.actionKey,
          generation: p.generation,
          ready: p.ready ?? true,
        }),
      { initialProps: initial },
    );

  beforeEach(() => localStorage.clear());

  it("stores a plan on this device only, for this game and seat", () => {
    const hook = setup({ actionKey: "k1", generation: 0 });
    act(() => hook.result.current.set(plan));
    expect(hook.result.current.plan).toEqual(plan);
    expect(loadPlan("g1", "b")?.plan).toEqual(plan);
    expect(localStorage.getItem(`${PREPARED_KEY_PREFIX}:g2:b`)).toBeNull();
    expect(loadPlan("g1", "c")).toBeNull();
  });

  it("restores after a reload for the same action and generation", () => {
    const first = setup({ actionKey: "k1", generation: 4 });
    act(() => first.result.current.set(plan));
    first.unmount();
    const second = setup({ actionKey: "k1", generation: 4 });
    expect(second.result.current.plan).toEqual(plan);
  });

  it("is dropped by a new strategic action", () => {
    const hook = setup({ actionKey: "k1", generation: 0 });
    act(() => hook.result.current.set(plan));
    hook.rerender({ actionKey: "k2", generation: 0 });
    expect(hook.result.current.plan).toBeNull();
    expect(loadPlan("g1", "b")).toBeNull();
  });

  it("is dropped when the action ended or was cancelled (none in progress)", () => {
    const hook = setup({ actionKey: "k1", generation: 0 });
    act(() => hook.result.current.set(plan));
    hook.rerender({ actionKey: null, generation: 0 });
    expect(hook.result.current.plan).toBeNull();
    expect(loadPlan("g1", "b")).toBeNull();
  });

  it("survives a history generation change while the same action is in progress (a committed batch replaces the session)", () => {
    const hook = setup({ actionKey: "k1", generation: 2 });
    act(() => hook.result.current.set(plan));
    hook.rerender({ actionKey: "k1", generation: 3 });
    expect(hook.result.current.plan).toEqual(plan);
    expect(loadPlan("g1", "b")?.plan).toEqual(plan);
  });

  it("is dropped when an undo, redo or restore removed the action from the log", () => {
    const hook = setup({ actionKey: "k1", generation: 2 });
    act(() => hook.result.current.set(plan));
    hook.rerender({ actionKey: null, generation: 3 });
    expect(hook.result.current.plan).toBeNull();
    expect(loadPlan("g1", "b")).toBeNull();
  });

  it("is kept while the log has not loaded, so a reload does not lose it", () => {
    const first = setup({ actionKey: "k1", generation: 0 });
    act(() => first.result.current.set(plan));
    first.unmount();
    const second = setup({ actionKey: null, generation: 0, ready: false });
    expect(loadPlan("g1", "b")?.plan).toEqual(plan);
    second.rerender({ actionKey: "k1", generation: 0, ready: true });
    expect(second.result.current.plan).toEqual(plan);
  });

  it("clear() revokes it", () => {
    const hook = setup({ actionKey: "k1", generation: 0 });
    act(() => hook.result.current.set(plan));
    act(() => hook.result.current.clear());
    expect(hook.result.current.plan).toBeNull();
    expect(loadPlan("g1", "b")).toBeNull();
  });

  it("works without storage and never stores for a spectator", () => {
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const hook = setup({ actionKey: "k1", generation: 0 });
    act(() => hook.result.current.set(plan));
    expect(hook.result.current.plan).toEqual(plan);
    spy.mockRestore();
    const spectator = setup({ actionKey: "k1", generation: 0, seat: null });
    act(() => spectator.result.current.set(plan));
    expect(spectator.result.current.plan).toBeNull();
  });
});
