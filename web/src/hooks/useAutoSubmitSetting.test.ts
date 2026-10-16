import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { AUTO_SUBMIT_KEY, useAutoSubmitSetting } from "./useAutoSubmitSetting.ts";

describe("useAutoSubmitSetting", () => {
  beforeEach(() => localStorage.clear());

  it("is on by default and reads a stored off", () => {
    expect(renderHook(() => useAutoSubmitSetting()).result.current.enabled).toBe(true);
    localStorage.setItem(AUTO_SUBMIT_KEY, "false");
    expect(renderHook(() => useAutoSubmitSetting()).result.current.enabled).toBe(false);
  });

  it("toggles, persists and stays in sync", () => {
    const a = renderHook(() => useAutoSubmitSetting());
    const b = renderHook(() => useAutoSubmitSetting());
    act(() => a.result.current.toggle());
    expect(localStorage.getItem(AUTO_SUBMIT_KEY)).toBe("false");
    expect(b.result.current.enabled).toBe(false);
  });
});
