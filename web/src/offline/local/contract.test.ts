import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GameSessionClient, type GameSessionState } from "../../protocol/client.ts";
import { LocalTransport } from "./LocalTransport.ts";
import { FakePort, GAME, SEAT, accepted, envelope, question, save, snapshot } from "./testKit.ts";

/**
 * `LocalTransport` and `GameSessionClient` must feed the shared reducer the same way: the same
 * messages in the same order give the same `GameSessionState` (apart from the connection fields).
 * The synthetic stream always runs; the recorded stream of a real hosted game runs when
 * TI4_HOSTED_STREAM is set (CI records it with `host.mjs --record`, see hostedStream.test.ts).
 */

class FakeWebSocket {
  static OPEN = 1;
  static CONNECTING = 0;
  static latest: FakeWebSocket;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  constructor(_url: string) {
    FakeWebSocket.latest = this;
  }
  send(): void {}
  close(): void {}
}

const comparable = (state: GameSessionState) => ({
  snapshot: state.snapshot,
  gameVersion: state.gameVersion,
  pendingChoice: state.pendingChoice,
  turnStatus: state.turnStatus,
  events: state.events,
  history: state.history,
  lastError: state.lastError,
});

/** Chunks of the flat stream as the engine delivers them: each question ends a chunk. */
function chunks(messages: unknown[]): unknown[][] {
  const out: unknown[][] = [];
  let current: unknown[] = [];
  for (const message of messages) {
    current.push(message);
    const m = message as { type?: string; pending_choice?: unknown };
    if ((m.type === "state_update" || m.type === "initial_snapshot") && m.pending_choice) {
      out.push(current);
      current = [];
    }
  }
  if (current.length) out.push(current);
  return out;
}

async function compare(gameId: string, messages: unknown[], seat: string) {
  vi.stubGlobal("WebSocket", FakeWebSocket);
  vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));
  const server = new GameSessionClient({ gameId, viewer: { role: "player", seat, playerSession: "x" } });
  server.start();
  FakeWebSocket.latest.readyState = 1;
  FakeWebSocket.latest.onopen?.();

  const port = new FakePort();
  const local = new LocalTransport({ gameId, config: { setup: {} }, createPort: async () => port });
  await local.begin();

  let compared = 0;
  for (const [index, chunk] of chunks(messages).entries()) {
    for (const message of chunk) FakeWebSocket.latest.onmessage?.({ data: JSON.stringify(message) } as MessageEvent);
    const last = index === chunks(messages).length - 1;
    port.emit(
      last && !(chunk.at(-1) as { pending_choice?: unknown }).pending_choice
        ? { t: "end", envelope: envelope("end", { messages: chunk, end: { finished: true, left: false, error: null, decisions: 0, answers: 0 } }), save: null }
        : { t: "ask", envelope: envelope("ask", { messages: chunk }), save: save() },
    );
    expect(comparable(local.getState())).toEqual(comparable(server.getState()));
    compared += 1;
  }
  server.stop();
  return { server: server.getState(), local: local.getState(), compared };
}

afterEach(() => vi.unstubAllGlobals());

describe("LocalTransport and GameSessionClient feed the reducer equivalently", () => {
  it("on a synthetic stream with an accepted answer, an event, an undo snapshot and game over", async () => {
    const event = {
      type: "event",
      protocol_version: 4,
      game_id: GAME,
      entry: { id: "e1", message: "A moved", version: 2, decision_count: 1 },
    };
    const stream = [
      snapshot(1, "n1"),
      accepted(1, "o1"),
      event,
      question(3, "n2"),
      snapshot(6, "n5", 1),
      accepted(6, "o2"),
      { type: "game_over", protocol_version: 4, game_id: GAME, game_version: 8, winner: SEAT, final_scores: { [SEAT]: 10 } },
    ];
    const { server, local, compared } = await compare(GAME, stream, SEAT);
    expect(compared).toBeGreaterThanOrEqual(3);
    expect(local.history.generation).toBe(1);
    expect(local.turnStatus).toEqual(server.turnStatus);
    expect(local.events.length).toBe(server.events.length);
  });

  const path = process.env.TI4_HOSTED_STREAM;
  it.skipIf(!path)("on the recorded stream of a hosted game (undo and rewind included)", async () => {
    const stream = JSON.parse(readFileSync(path as string, "utf8")) as { game_id: string; messages: unknown[] };
    const { server, local, compared } = await compare(stream.game_id, stream.messages, "a");
    expect(compared).toBeGreaterThan(40);
    expect(local.events.length).toBeGreaterThan(50);
    expect(local.history.generation).toBe(1);
    expect(local.lastError).toBeNull();
    expect(local.events.length).toBe(server.events.length);
  });
});
