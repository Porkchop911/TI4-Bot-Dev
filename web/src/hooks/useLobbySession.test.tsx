import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useLobbySession } from "./useLobbySession.ts";

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
const json = (value: unknown) => ({ ok: true, json: async () => value });

afterEach(() => {
  vi.unstubAllGlobals();
});

it("serializes lobby mutations, keeps failed actions actionable, and permits a retry", async () => {
  let finish!: (response: unknown) => void;
  const fetchMock = vi.fn().mockImplementation((url: string) => {
    if (url.endsWith("/map"))
      return new Promise((resolve) => {
        finish = resolve;
      });
    return Promise.resolve(json({ player: { id: "player_a" }, lobby }));
  });
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await waitFor(() => expect(result.current.playerId).toBe("player_a"));
  let first!: Promise<void>;
  let blocked!: Promise<void>;
  act(() => {
    first = result.current.chooseMap({ kind: "random" });
    blocked = result.current.start();
  });
  expect(result.current.pendingAction).toBe("map");
  expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/start"))).toHaveLength(0);
  await act(async () => {
    finish({ ok: false, status: 409, text: async () => "Game already running" });
    await Promise.all([first, blocked]);
  });
  expect(result.current.pendingAction).toBeNull();
  expect(result.current.error).toContain("409");
  act(() => {
    first = result.current.chooseMap({ kind: "random" });
  });
  await act(async () => {
    finish(json({ ...lobby, lobby_version: 9 }));
    await first;
  });
  expect(result.current.error).toBeNull();
  expect(result.current.lobby?.lobby_version).toBe(9);
});

it("posts the host's map choice with the credential and adopts the lobby it returns", async () => {
  const chosen = {
    ...lobby,
    lobby_version: 2,
    map: { kind: "random", systems: 36, hyperlanes: false, recommended: false },
    map_revision: 1,
  };
  const fetchMock = vi
    .fn()
    .mockImplementation((url: string) =>
      Promise.resolve(
        String(url).endsWith("/map") ? json(chosen) : json({ player: { id: "player_a" }, lobby }),
      ),
    );
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await waitFor(() => expect(result.current.playerId).toBe("player_a"));
  await act(async () => {
    await result.current.chooseMap({ kind: "random" });
  });
  const call = fetchMock.mock.calls.find(([url]) => String(url).endsWith("/lobby/map"))!;
  expect(call[1].method).toBe("POST");
  expect(call[1].headers["x-ti4-player-session"]).toBe("session_secret");
  expect(JSON.parse(call[1].body)).toEqual({ map: { kind: "random" } });
  expect(result.current.lobby?.map?.kind).toBe("random");
  expect(result.current.lobby?.map_revision).toBe(1);
});

it("keeps background poll failures out of `error`, shows connectionLost only after repeated failures, and recovers", async () => {
  vi.useFakeTimers();
  let failing = 0;
  const fetchMock = vi.fn().mockImplementation(() => {
    if (failing > 0) {
      failing--;
      return Promise.reject(new TypeError("Failed to fetch"));
    }
    return Promise.resolve(json({ player: { id: "player_a" }, lobby }));
  });
  vi.stubGlobal("fetch", fetchMock);
  try {
    const { result } = renderHook(() => useLobbySession("game", "session_secret"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.playerId).toBe("player_a");
    failing = 1;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(result.current.error).toBeNull();
    expect(result.current.connectionLost).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(result.current.error).toBeNull();
    failing = 3;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6_000);
    });
    expect(result.current.error).toBeNull();
    expect(result.current.connectionLost).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(result.current.connectionLost).toBe(false);
  } finally {
    vi.useRealTimers();
  }
});

it("does not stack lobby polls while one is still in flight", async () => {
  vi.useFakeTimers();
  try {
    let finish!: (response: unknown) => void;
    const fetchMock = vi.fn().mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const { result } = renderHook(() => useLobbySession("game", "session_secret"));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // A slow server: several poll ticks pass while the first join is unanswered.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish(json({ player: { id: "player_a" }, lobby }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.playerId).toBe("player_a");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  } finally {
    vi.useRealTimers();
  }
});

it("sends the chosen faction with an added bot, changes it later, and surfaces a refusal", async () => {
  const sent: { url: string; body: unknown }[] = [];
  let botCalls = 0;
  const fetchMock = vi.fn().mockImplementation((url: string, init?: { body?: string }) => {
    if (init?.body && /\/(bots|seat-faction)$/.test(url)) sent.push({ url, body: JSON.parse(init.body) });
    if (url.endsWith("/bots") && ++botCalls > 1)
      return Promise.resolve({
        ok: false,
        status: 409,
        text: async () => "Faction 'sol' is already taken by Rule bot 1",
      });
    return Promise.resolve(json({ player: { id: "player_a" }, lobby }));
  });
  vi.stubGlobal("fetch", fetchMock);
  const { result } = renderHook(() => useLobbySession("game", "session_secret"));
  await waitFor(() => expect(result.current.playerId).toBe("player_a"));
  await act(async () => {
    await result.current.addRandomBots({ count: 1, kind: "smart", faction: "sol" });
  });
  expect(sent[0]).toEqual({
    url: "/api/games/game/lobby/bots",
    body: { kind: "smart", count: 1, faction: "sol" },
  });
  await act(async () => {
    await result.current.setSeatFaction("player_b", "hacan");
    await result.current.setSeatFaction("player_b", null);
  });
  expect(sent[1]).toEqual({
    url: "/api/games/game/lobby/seat-faction",
    body: { player_id: "player_b", faction: "hacan" },
  });
  expect(sent[2].body).toEqual({ player_id: "player_b", faction: null });
  await act(async () => {
    await result.current.addRandomBots({ count: 1, kind: "smart", faction: "sol" });
  });
  expect(result.current.error).toContain("already taken by Rule bot 1");
});
