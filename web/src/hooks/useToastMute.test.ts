import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useToastMute, TOAST_MUTE_KEY } from "./useToastMute.ts";

describe("useToastMute", () => {
  beforeEach(() => localStorage.clear());

  it("starts unmuted and reads a stored choice", () => {
    expect(renderHook(() => useToastMute()).result.current.muted).toBe(false);
    localStorage.setItem(TOAST_MUTE_KEY, "true");
    expect(renderHook(() => useToastMute()).result.current.muted).toBe(true);
  });

  it("toggles, persists and keeps every user of the hook in sync", () => {
    const a = renderHook(() => useToastMute());
    const b = renderHook(() => useToastMute());
    act(() => {
      expect(a.result.current.toggleMute()).toBe(true);
    });
    expect(localStorage.getItem(TOAST_MUTE_KEY)).toBe("true");
    expect(a.result.current.muted).toBe(true);
    expect(b.result.current.muted).toBe(true);
    act(() => a.result.current.setMuted(false));
    expect(b.result.current.muted).toBe(false);
  });
});
