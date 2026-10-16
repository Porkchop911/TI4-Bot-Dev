import { afterEach, describe, expect, it, vi } from "vitest";
import { GameSessionClient, RECONNECT_GIVE_UP_AFTER } from "./client.ts";
import { InitialSnapshotMsg, PROTOCOL_VERSION } from "./types.ts";

class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static all: FakeWebSocket[] = [];
  readyState = FakeWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((event?: { code: number }) => void) | null = null;
  sent: string[] = [];
  constructor(_url: string) {
    FakeWebSocket.all.push(this);
  }
  send(value: string): void {
    this.sent.push(value);
  }
  close(): void {
    this.readyState = 3;
    this.onclose?.();
  }
  open(): void {
    this.readyState = FakeWebSocket.OPEN;
    this.onopen?.();
  }
  deliver(message: object): void {
    this.onmessage?.({ data: JSON.stringify(message) } as MessageEvent);
  }
  static get latest(): FakeWebSocket {
    return FakeWebSocket.all[FakeWebSocket.all.length - 1];
  }
}

const pending = (nonce: string) => ({
  nonce,
  choice: {
    player: "player_a",
    prompt: "Choose",
    context: { subtype: "movement_step" },
    options: [{ id: "o", label: "O" }],
  },
});

const snapshot = (version: number, nonce: string | null = "n1"): InitialSnapshotMsg => ({
  type: "initial_snapshot",
  protocol_version: PROTOCOL_VERSION,
  game_id: "game_12345",
  game_version: version,
  viewer: { role: "player", seat: "player_a" },
  state: {},
  galaxy_layout: { version: 1, active_sources: [], placements: [] },
  view: {
    round: 1,
    phase: "action",
    speaker: "player_a",
    seating_order: ["player_a"],
    finished: false,
    players: [],
    board: { systems: {} },
    table: {
      revealed_objectives: [],
      scored_objectives: {},
      unclaimed_strategy_cards: [],
      strategy_card_goods: {},
      laws: {},
    },
  },
  turn_status: { kind: "active_turn", player: "player_a", phase: "action", round: 1 },
  pending_choice: nonce ? (pending(nonce) as never) : null,
  events: [],
});

const ok = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
const status = (code: number, body = "") => new Response(body, { status: code });

function newClient() {
  return new GameSessionClient({
    gameId: "game_12345",
    viewer: { role: "player", seat: "player_a", playerSession: "private" },
  });
}

afterEach(() => {
  FakeWebSocket.all = [];
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("GameSessionClient resume after idle", () => {
  it("keeps the last game on screen while reconnecting and adopts the fresh state on return", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => ok(snapshot(4))));
    const client = newClient();
    client.start();
    FakeWebSocket.latest.open();
    await vi.advanceTimersByTimeAsync(0);
    expect(client.getState().gameVersion).toBe(4);
    const first = FakeWebSocket.latest;
    first.onclose?.();
    expect(client.getState().status).toBe("disconnected");
    expect(client.getState().snapshot).not.toBeNull();
    expect(client.getState().pendingChoice?.nonce).toBe("n1");
    expect(client.getState().lastError).toBeNull();
    // Someone else acted while this phone slept.
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => ok(snapshot(9, "n7"))));
    await vi.advanceTimersByTimeAsync(1_000);
    expect(FakeWebSocket.latest).not.toBe(first);
    FakeWebSocket.latest.open();
    await vi.advanceTimersByTimeAsync(0);
    expect(client.getState().status).toBe("connected");
    expect(client.getState().gameVersion).toBe(9);
    expect(client.getState().pendingChoice?.nonce).toBe("n7");
    client.stop();
  });

  it("replaces a zombie socket when the page returns after a long absence, not after a short one", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => ok(snapshot(4))));
    const client = newClient();
    client.start();
    const zombie = FakeWebSocket.latest;
    zombie.open();
    client.resume({ reason: "focus", awayMs: 500 });
    expect(FakeWebSocket.latest).toBe(zombie);
    client.resume({ reason: "visible", awayMs: 30 * 60_000 });
    expect(FakeWebSocket.latest).not.toBe(zombie);
    expect(zombie.readyState).toBe(3);
    expect(client.getState().status).toBe("connecting");
    client.stop();
  });

  it("retries the snapshot through transient failures without ever setting an error", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockResolvedValueOnce(status(502))
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockImplementation(async () => ok(snapshot(6)));
    vi.stubGlobal("fetch", fetchMock);
    const client = newClient();
    client.start();
    for (let i = 0; i < 6; i++) {
      await vi.advanceTimersByTimeAsync(1_000);
      expect(client.getState().lastError).toBeNull();
    }
    expect(fetchMock).toHaveBeenCalledTimes(4);
    expect(client.getState().gameVersion).toBe(6);
    client.stop();
  });

  it("recovers after the server was down for five reconnect attempts", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const client = newClient();
    client.start();
    for (let i = 0; i < 5; i++) {
      FakeWebSocket.latest.onclose?.();
      await vi.advanceTimersByTimeAsync(15_000);
    }
    expect(client.getState().fatal).toBeNull();
    expect(client.getState().lastError).toBeNull();
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => ok(snapshot(12))));
    FakeWebSocket.latest.open();
    FakeWebSocket.latest.deliver(snapshot(12));
    expect(client.getState().status).toBe("connected");
    expect(client.getState().gameVersion).toBe(12);
    client.stop();
  });

  it("gives up after too many failed reconnects with a plain message, and Retry starts over", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const client = newClient();
    client.start();
    for (let i = 0; i < RECONNECT_GIVE_UP_AFTER + 1; i++) {
      FakeWebSocket.latest.onclose?.();
      await vi.advanceTimersByTimeAsync(16_000);
    }
    const fatal = client.getState().fatal;
    expect(fatal?.kind).toBe("unreachable");
    expect(fatal?.message).not.toMatch(/TypeError|Failed to fetch|lobby/i);
    const sockets = FakeWebSocket.all.length;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeWebSocket.all.length).toBe(sockets); // no more automatic attempts
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => ok(snapshot(3))));
    client.retryNow();
    expect(client.getState().fatal).toBeNull();
    expect(FakeWebSocket.all.length).toBe(sockets + 1);
    client.stop();
  });

  it("reports a game that is gone (404) as such", async () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal("fetch", vi.fn().mockImplementation(async () => status(404, "no such game")));
    const client = newClient();
    client.start();
    await vi.waitFor(() => expect(client.getState().fatal?.kind).toBe("gone"));
    expect(client.getState().fatal?.message).toMatch(/not found/i);
    client.stop();
  });

  it("does not retry a real server error on the snapshot (403 stays visible)", async () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    const fetchMock = vi.fn().mockImplementation(async () => status(403, "bad session"));
    vi.stubGlobal("fetch", fetchMock);
    const client = newClient();
    client.start();
    await vi.waitFor(() => expect(client.getState().lastError).toMatch(/403/));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    client.stop();
  });

  it("checks the server instead of re-sending when a batch POST dies on the network", async () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    const calls: string[] = [];
    let applied = false;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push(`${init?.method ?? "GET"} ${url.split("/").pop()}`);
        if (init?.method === "POST") throw new TypeError("Failed to fetch");
        return ok(applied ? snapshot(5, null) : snapshot(4, "n1"));
      }),
    );
    const client = newClient();
    client.start();
    await vi.waitFor(() => expect(client.getState().pendingChoice?.nonce).toBe("n1"));
    const plan = { kind: "tactical_movement", destination: "s1", steps: [] } as never;
    // The request never reached the server: report it, do not claim success.
    await expect(client.submitBatch(plan)).rejects.toThrow(/Nothing was applied/);
    expect(calls.filter((c) => c.startsWith("POST"))).toHaveLength(1);
    // The request reached the server but the answer was lost: the fresh state shows it applied.
    applied = true;
    await expect(client.submitBatch(plan)).resolves.toBeUndefined();
    expect(client.getState().gameVersion).toBe(5);
    client.stop();
  });
});
