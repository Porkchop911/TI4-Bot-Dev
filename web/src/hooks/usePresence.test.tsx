import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { usePresence } from "./usePresence.ts";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it("does not stack heartbeats while one is still in flight", async () => {
  vi.useFakeTimers();
  let finish!: (response: unknown) => void;
  const fetchMock = vi.fn().mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  vi.stubGlobal("fetch", fetchMock);
  const onInvalid = vi.fn();
  renderHook(() => usePresence("game", "session_secret", onInvalid));
  expect(fetchMock).toHaveBeenCalledTimes(1);
  // A slow server: time passes (short of the 10 s timeout that replaces a stuck request) while the first heartbeat is unanswered; a tab switch deliberately replaces it (resume).
  await act(async () => {
    await vi.advanceTimersByTimeAsync(9_000);
  });
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await act(async () => {
    finish({ status: 204 });
    await vi.advanceTimersByTimeAsync(0);
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10_000);
  });
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(onInvalid).not.toHaveBeenCalled();
});
