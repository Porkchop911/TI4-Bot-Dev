import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBackUndo } from "./useBackUndo.ts";

const pop = (state: unknown = null) => act(() => void window.dispatchEvent(new PopStateEvent("popstate", { state })));

describe("useBackUndo", () => {
  let push: ReturnType<typeof vi.spyOn>;
  let back: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    window.history.replaceState(null, "");
    push = vi.spyOn(window.history, "pushState");
    back = vi.spyOn(window.history, "back").mockImplementation(() => {});
  });
  afterEach(() => vi.restoreAllMocks());

  it("does nothing in a game with other people", () => {
    renderHook(() => useBackUndo({ enabled: false, canUndo: true, busy: false, undo: vi.fn() }));
    expect(push).not.toHaveBeenCalled();
    pop();
    expect(back).not.toHaveBeenCalled();
  });

  it("pushes one sentinel, not a second after a reload (the entry already has it)", () => {
    const opts = { enabled: true, canUndo: true, busy: false, undo: vi.fn() };
    const first = renderHook(() => useBackUndo(opts));
    expect(push).toHaveBeenCalledTimes(1);
    first.unmount();
    window.history.replaceState({ ti4BackUndo: true }, "");
    push.mockClear();
    renderHook(() => useBackUndo(opts));
    expect(push).not.toHaveBeenCalled();
  });

  it("Back with something to undo undoes one decision, stays on the page and says so", async () => {
    const undo = vi.fn().mockResolvedValue(true);
    const { result } = renderHook(() => useBackUndo({ enabled: true, canUndo: true, busy: false, undo }));
    push.mockClear();
    pop();
    await act(async () => {});
    expect(undo).toHaveBeenCalledTimes(1);
    expect(back).not.toHaveBeenCalled();
    expect(push).toHaveBeenCalledTimes(1); // the sentinel is back
    expect(result.current).toBe("Undid your last decision (Back)");
  });

  it("Back with nothing to undo leaves the game screen once", () => {
    const undo = vi.fn();
    renderHook(() => useBackUndo({ enabled: true, canUndo: false, busy: false, undo }));
    push.mockClear();
    pop();
    expect(undo).not.toHaveBeenCalled();
    expect(back).toHaveBeenCalledTimes(1);
  });

  it("ignores Forward onto the sentinel and keeps the player while a change runs", () => {
    const undo = vi.fn();
    renderHook(() => useBackUndo({ enabled: true, canUndo: true, busy: true, undo }));
    push.mockClear();
    pop({ ti4BackUndo: true });
    expect(undo).not.toHaveBeenCalled();
    expect(back).not.toHaveBeenCalled();
    pop();
    expect(undo).not.toHaveBeenCalled();
    expect(push).toHaveBeenCalledTimes(1);
    expect(back).not.toHaveBeenCalled();
  });
});
