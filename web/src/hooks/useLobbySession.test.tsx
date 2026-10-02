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
      ready: false,
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "slot_2",
      position: 2,
      occupant: null,
      nickname: null,
      ready: false,
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
    if (url.endsWith("/ready"))
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
    first = result.current.setReady(true);
    blocked = result.current.start();
  });
  expect(result.current.pendingAction).toBe("ready");
  expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/start"))).toHaveLength(0);
  await act(async () => {
    finish({ ok: false, status: 409, text: async () => "Player already ready" });
    await Promise.all([first, blocked]);
  });
  expect(result.current.pendingAction).toBeNull();
  expect(result.current.error).toContain("409");
  act(() => {
    first = result.current.setReady(true);
  });
  await act(async () => {
    finish(json({ ...lobby, slots: [{ ...lobby.slots[0], ready: true }, lobby.slots[1]] }));
    await first;
  });
  expect(result.current.error).toBeNull();
  expect(result.current.lobby?.slots[0].ready).toBe(true);
});
