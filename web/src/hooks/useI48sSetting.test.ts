import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { ASK_I48S_KEY, readAskI48s, useI48sSetting } from "./useI48sSetting.ts";
import { i48sDeclineOption } from "../presentation/i48sOffer.ts";
import { useI48sAutoDecline, I48S_DECLINE_DELAY_MS } from "./useI48sAutoDecline.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";

const offer = (actor = "p1", nonce = "n1"): PendingChoiceDto =>
  ({
    prompt: "I48S: exhaust to let p2 replace 1 infantry in the active system with a mech",
    actor,
    nonce,
    context: { subtype: "l1z1x_agent_swap" },
    options: [
      { id: "use", label: "exhaust the agent", kind: "leader" },
      { id: "decline", label: "decline", kind: "decline" },
    ],
  }) as unknown as PendingChoiceDto;

describe("I48S setting", () => {
  beforeEach(() => localStorage.clear());

  it("asks by default, reads a stored off, toggles and persists", () => {
    expect(readAskI48s()).toBe(true);
    const a = renderHook(() => useI48sSetting());
    const b = renderHook(() => useI48sSetting());
    act(() => a.result.current.toggle());
    expect(localStorage.getItem(ASK_I48S_KEY)).toBe("false");
    expect(b.result.current.ask).toBe(false);
    act(() => b.result.current.toggle());
    expect(a.result.current.ask).toBe(true);
  });

  it("finds the decline only for the owner's own I48S offer", () => {
    expect(i48sDeclineOption(offer(), "p1")).toBe("decline");
    expect(i48sDeclineOption(offer(), "p2")).toBeNull();
    expect(i48sDeclineOption(offer(), null)).toBeNull();
    const other = { ...offer(), context: { subtype: "decision" } } as unknown as PendingChoiceDto;
    expect(i48sDeclineOption(other, "p1")).toBeNull();
  });

  it("declines once when off and the game moved forward; asks after a load or when on", () => {
    vi.useFakeTimers();
    const submit = vi.fn().mockResolvedValue(undefined);
    const h = (cursor: number, choice: PendingChoiceDto | null) =>
      ({ choice, viewerSeat: "p1", history: { cursor, redo_count: 0, generation: 0 }, submit }) as never;
    // On: asks.
    let r = renderHook((p) => useI48sAutoDecline(p), { initialProps: h(1, null) });
    r.rerender(h(2, offer("p1", "a")));
    act(() => void vi.advanceTimersByTime(I48S_DECLINE_DELAY_MS + 10));
    expect(submit).not.toHaveBeenCalled();
    // Off: declines when reached by moving forward.
    localStorage.setItem(ASK_I48S_KEY, "false");
    r = renderHook((p) => useI48sAutoDecline(p), { initialProps: h(2, null) });
    r.rerender(h(3, offer("p1", "b")));
    act(() => void vi.advanceTimersByTime(I48S_DECLINE_DELAY_MS + 10));
    expect(submit).toHaveBeenCalledWith("decline", "b");
    // Off but opened by a page load (no forward move): asked.
    submit.mockClear();
    renderHook((p) => useI48sAutoDecline(p), { initialProps: h(3, offer("p1", "c")) });
    act(() => void vi.advanceTimersByTime(I48S_DECLINE_DELAY_MS + 10));
    expect(submit).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
