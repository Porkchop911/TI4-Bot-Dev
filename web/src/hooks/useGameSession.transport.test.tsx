// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SERVER_CAPABILITIES } from "../protocol/capabilities.ts";
import { GameSessionClient, type GameSessionState } from "../protocol/client.ts";
import { MessageDrivenState, type GameTransport } from "../protocol/transport.ts";
import type { ServerMessage } from "../protocol/types.ts";
import { useGameSession } from "./useGameSession.ts";

const blank: GameSessionState = {
  status: "connecting",
  gameVersion: 0,
  snapshot: null,
  pendingChoice: null,
  turnStatus: null,
  lastError: null,
  fatal: null,
  reconnecting: false,
  events: [],
  history: { cursor: 0, redo_count: 0, generation: 0 },
};

/** A transport with no network: tests push state in and observe the commands the hook sends. */
function fakeTransport(capabilities = { ...SERVER_CAPABILITIES, multiplayer: false, turnRedo: false }) {
  const store = new MessageDrivenState(blank);
  const calls: string[] = [];
  const transport: GameTransport = {
    capabilities,
    getState: () => store.get(),
    subscribe: (l) => store.subscribe(l),
    start: () => {
      calls.push("start");
      store.set({ ...store.get(), status: "connected" });
    },
    stop: () => void calls.push("stop"),
    retryNow: () => void calls.push("retry"),
    submitChoice: async (id) => void calls.push(`choice:${id}`),
    submitBatch: async () => void calls.push("batch"),
    submitMovementBatch: async (d) => void calls.push(`move:${d}`),
    resumeBatch: async () => void calls.push("resumeBatch"),
    dismissBatchResume: () => void calls.push("dismissBatch"),
    setReactionMode: (c, m) => void calls.push(`mode:${c}:${m}`),
    setReactionIntent: (t) => void calls.push(`intent:${t.join(",")}`),
    passReactionHold: () => void calls.push("pass"),
    changeHistory: async () => void calls.push("history"),
    fetchReplay: async () => ({ text: "{}", filename: "r.json" }),
    queryReach: async () => ({ ok: false, reason: "x" }) as never,
    previewSecondary: async () => ({ ok: false, reason: "x" }) as never,
    fetchTurnRedoStatus: async () => null,
    turnRedoCommand: async () => void calls.push("redo"),
  };
  return { transport, store, calls };
}

const options = { gameId: "g", viewer: { role: "spectator" } as const };

describe("useGameSession with a GameTransport", () => {
  it("GameSessionClient satisfies the interface and offers every capability", () => {
    const client: GameTransport = new GameSessionClient({ gameId: "g", viewer: { role: "spectator" } });
    expect(client.capabilities).toEqual(SERVER_CAPABILITIES);
    expect(Object.values(client.capabilities).every(Boolean)).toBe(true);
  });

  it("starts the injected transport, exposes its capabilities and stops it on unmount", () => {
    const { transport, calls } = fakeTransport();
    const { result, unmount } = renderHook(() =>
      useGameSession({ ...options, createTransport: () => transport }),
    );
    expect(calls).toEqual(["start"]);
    expect(result.current.status).toBe("connected");
    expect(result.current.capabilities.multiplayer).toBe(false);
    unmount();
    expect(calls).toEqual(["start", "stop"]);
  });

  it("re-renders when ServerMessage objects go through the shared reducer", () => {
    const { transport, store } = fakeTransport();
    const { result } = renderHook(() =>
      useGameSession({ ...options, createTransport: () => transport }),
    );
    const message = {
      type: "error",
      protocol_version: 1,
      code: "bad",
      message: "nope",
    } as unknown as ServerMessage;
    const before = result.current.gameVersion;
    act(() => store.ingest(message));
    expect(result.current.gameVersion).toBe(before);
    act(() => store.set({ ...store.get(), lastError: "boom", gameVersion: 7 }));
    expect(result.current.lastError).toBe("boom");
    expect(result.current.gameVersion).toBe(7);
  });

  it("routes commands to the transport", async () => {
    const { transport, calls } = fakeTransport();
    const { result } = renderHook(() =>
      useGameSession({ ...options, createTransport: () => transport }),
    );
    await act(async () => {
      await result.current.submitChoice("opt-1");
      await result.current.submitMovementBatch("sys", []);
      result.current.setReactionMode("Sabotage", "never" as never);
      result.current.retryConnection();
      result.current.dismissBatchResume();
    });
    expect(calls).toEqual([
      "start",
      "choice:opt-1",
      "move:sys",
      "mode:Sabotage:never",
      "retry",
      "dismissBatch",
    ]);
  });

  it("keeps command identities stable across renders", () => {
    const { transport } = fakeTransport();
    const { result, rerender } = renderHook(() =>
      useGameSession({ ...options, createTransport: () => transport }),
    );
    const { queryReach, submitChoice, fetchTurnRedoStatus } = result.current;
    rerender();
    expect(result.current.queryReach).toBe(queryReach);
    expect(result.current.submitChoice).toBe(submitChoice);
    expect(result.current.fetchTurnRedoStatus).toBe(fetchTurnRedoStatus);
  });

  it("builds the transport once", () => {
    const { transport } = fakeTransport();
    const create = vi.fn(() => transport);
    const { rerender } = renderHook(() => useGameSession({ ...options, createTransport: create }));
    rerender();
    expect(create).toHaveBeenCalledTimes(1);
  });
});
