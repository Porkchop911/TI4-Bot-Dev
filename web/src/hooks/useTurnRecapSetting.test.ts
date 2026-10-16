import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useTurnRecapSetting, TURN_RECAP_KEY } from "./useTurnRecapSetting.ts";

describe("useTurnRecapSetting", () => {
  beforeEach(() => localStorage.clear());

  it("is off by default and reads a stored choice", () => {
    expect(renderHook(() => useTurnRecapSetting()).result.current.enabled).toBe(false);
    localStorage.setItem(TURN_RECAP_KEY, "true");
    expect(renderHook(() => useTurnRecapSetting()).result.current.enabled).toBe(true);
  });

  it("toggles, persists and keeps every user of the hook in sync", () => {
    const a = renderHook(() => useTurnRecapSetting());
    const b = renderHook(() => useTurnRecapSetting());
    act(() => {
      expect(a.result.current.toggle()).toBe(true);
    });
    expect(localStorage.getItem(TURN_RECAP_KEY)).toBe("true");
    expect(b.result.current.enabled).toBe(true);
    act(() => a.result.current.setEnabled(false));
    expect(b.result.current.enabled).toBe(false);
  });

  it("survives unavailable storage", () => {
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { result } = renderHook(() => useTurnRecapSetting());
    expect(result.current.enabled).toBe(false);
    expect(() => act(() => result.current.setEnabled(true))).not.toThrow();
    get.mockRestore();
    set.mockRestore();
  });
});
