import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { LOBBY_POLL_FAILURE_LIMIT, LOBBY_POLL_TIMEOUT_MS, useLobbySession } from "./useLobbySession.ts";
import { usePresence } from "./usePresence.ts";

const lobby = {
  game_id: "game",
  phase: "lobby",
  lobby_version: 1,
  host_player_id: "player_a",
  slots: [
    {
      slot_id: "slot_1",
      position: 1,
      occupant: "player_a",
      nickname: "Host",
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "slot_2",
      position: 2,
      occupant: null,
      nickname: null,
      connected: false,
      can_take_over: false,
    },
  ],
};
const json = (value: unknown) => ({ ok: true, status: 200, json: async () => value });

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const comeBack = async (awayMs: number) => {
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(awayMs);
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(10);
};

it("lobby: the first failed fetches after a sleep stay 'loading', never an error, and recovery is silent", async () => {
  vi.useFakeTimers();
  const fetchMock = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("Failed to fetch"))
    .mockRejectedValueOnce(new TypeError("Failed to fetch"))
    .mockResolvedValue(json({ player: { id: "player_a" }, lobby }));
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10);
  });
  expect(result.current.loading).toBe(true);
  expect(result.current.error).toBeNull();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(2_000);
  });
  expect(result.current.loading).toBe(true);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(2_000);
  });
  expect(result.current.lobby?.phase).toBe("lobby");
  expect(result.current.loading).toBe(false);
  expect(result.current.error).toBeNull();
  expect(result.current.connectionLost).toBe(false);
});

it("lobby: after the failure limit it stops 'loading' so the page can offer Retry, and Retry polls at once", async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(2_000 * LOBBY_POLL_FAILURE_LIMIT);
  });
  expect(result.current.lobby).toBeNull();
  expect(result.current.loading).toBe(false);
  expect(result.current.connectionLost).toBe(true);
  expect(result.current.error).toBeNull();
  fetchMock.mockResolvedValue(json({ player: { id: "player_a" }, lobby }));
  await act(async () => {
    result.current.retry();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(result.current.lobby?.phase).toBe("lobby");
  expect(result.current.connectionLost).toBe(false);
});

it("lobby: a 404 means the game is gone", async () => {
  const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 404, text: async () => "" });
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await vi.waitFor(() => expect(result.current.gone).toBe(true));
  expect(result.current.error).toBeNull();
});

it("lobby: coming back replaces a poll stuck on a dead connection instead of waiting for it", async () => {
  vi.useFakeTimers();
  const signals: AbortSignal[] = [];
  const fetchMock = vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
    if (signals.length === 0) {
      if (init?.signal) signals.push(init.signal);
      return new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
      });
    }
    return Promise.resolve(json({ player: { id: "player_a" }, lobby }));
  });
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1_000);
  });
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await act(async () => {
    await comeBack(1_000);
  });
  expect(signals[0].aborted).toBe(true);
  expect(result.current.lobby?.phase).toBe("lobby");
});

it("lobby: a poll with no answer is abandoned after the timeout so polling continues", async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn().mockImplementation(
    (_url: string, init?: RequestInit) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
      }),
  );
  vi.stubGlobal("fetch", fetchMock);
  renderHook(() => useLobbySession("game", "session_secret"));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(LOBBY_POLL_TIMEOUT_MS + 2_100);
  });
  expect(fetchMock.mock.calls.length).toBeGreaterThanOrEqual(2);
  expect(fetchMock.mock.calls.length).toBeLessThanOrEqual(3);
});

it("lobby: a failed action says it cannot reach the server, not 'TypeError: Failed to fetch'", async () => {
  const fetchMock = vi.fn().mockResolvedValue(json({ player: { id: "player_a" }, lobby }));
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await vi.waitFor(() => expect(result.current.lobby).not.toBeNull());
  fetchMock.mockRejectedValue(new TypeError("Failed to fetch"));
  await act(async () => {
    await result.current.start();
  });
  expect(result.current.error).toMatch(/cannot reach the server/i);
  expect(result.current.error).not.toMatch(/TypeError|Failed to fetch|check the lobby/i);
});

it("presence: does not stack heartbeats, and a return from sleep replaces the stuck one", async () => {
  vi.useFakeTimers();
  const signals: AbortSignal[] = [];
  const fetchMock = vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
    if (init?.signal) signals.push(init.signal);
    return new Promise((resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
      void resolve;
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  const onInvalid = vi.fn();
  renderHook(() => usePresence("game", "secret", onInvalid));
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(5_000);
  });
  expect(fetchMock).toHaveBeenCalledTimes(1); // still in flight: ticks are skipped
  await act(async () => {
    await comeBack(1_000);
  });
  expect(signals[0].aborted).toBe(true);
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(onInvalid).not.toHaveBeenCalled();
});

it("presence: a 403 still reports an invalid credential; a network error does not", async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
  vi.stubGlobal("fetch", fetchMock);
  const onInvalid = vi.fn();
  renderHook(() => usePresence("game", "secret", onInvalid));
  await act(async () => {
    await vi.advanceTimersByTimeAsync(30_000);
  });
  expect(onInvalid).not.toHaveBeenCalled();
  fetchMock.mockResolvedValue({ status: 403 });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10_000);
  });
  expect(onInvalid).toHaveBeenCalled();
});
