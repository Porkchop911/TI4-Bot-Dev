import { afterEach, describe, expect, it, vi } from "vitest";
import {
  GameSessionClient,
  GameSessionState,
  historyFailureMessage,
  isHistoryVersionConflict,
  PREVIEW_TIMEOUT_MS,
  PreviewUnsupportedError,
  ReachUnsupportedError,
  reduceServerMessage,
  serverEventLog,
} from "./client.ts";
import { decodeCreateGameResponse, decodeJoinResponse, decodeLobby } from "./decode.ts";
import { decodeServerMessage } from "@ti4/shared/wire/decode.ts";
import { PROTOCOL_VERSION, type ServerMessage } from "./types.ts";
import { validNickname } from "./nickname.ts";

const snapshot: Extract<ServerMessage, { type: "initial_snapshot" }> = {
  type: "initial_snapshot",
  protocol_version: PROTOCOL_VERSION,
  game_id: "game_12345",
  game_version: 4,
  viewer: { role: "spectator" },
  state: {},
  galaxy_layout: { version: 1, active_sources: [], placements: [] },
  view: {
    round: 1,
    phase: "strategy",
    speaker: "seat_a",
    seating_order: ["seat_a"],
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
    turn: 0,
  },
  turn_status: {
    kind: "active_turn",
    player: "seat_a",
    phase: "strategy",
    round: 1,
  },
  events: [],
};

const state: GameSessionState = {
  status: "connected",
  gameVersion: 0,
  snapshot: null,
  pendingChoice: null,
  turnStatus: null,
  lastError: null,
  events: [],
  history: { cursor: 0, redo_count: 0, generation: 0 },
};

describe("GameSessionClient reducer", () => {
  it("clears a stale card offer when a nested window moves to another seat", () => {
    const previous = {
      ...state,
      gameVersion: 5,
      pendingChoice: {
        actor: "seat_a",
        nonce: "old",
        prompt: "Play Shields Holding",
        options: [
          {
            id: "reaction:seat_a:HITS_TO_ASSIGN:when",
            label: "Play Shields Holding",
            kind: "",
            payload: {},
          },
        ],
      },
    };
    const next = reduceServerMessage(previous, {
      type: "turn_status",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      game_version: 6,
      status: {
        kind: "waiting_for_decision",
        seat: "seat_b",
        phase: "action",
        round: 1,
        stage: "Waiting for player",
      },
    });
    expect(next.pendingChoice).toBeNull();
    expect(next.turnStatus).toMatchObject({ seat: "seat_b" });
  });

  it("shows a game_over push: the status turns to game over and no choice stays open", () => {
    const open = {
      ...state,
      pendingChoice: {
        player: "seat_a",
        prompt: "end your turn",
        options: [{ id: "end", label: "end your turn", kind: "", payload: {} }],
      },
    } as unknown as GameSessionState;
    const next = reduceServerMessage(open, {
      type: "game_over",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      game_version: 9,
      winner: "seat_b",
      final_scores: { seat_a: 8, seat_b: 10 },
    });
    expect(next.turnStatus).toEqual({ kind: "game_over", winner: "seat_b" });
    expect(next.pendingChoice).toBeNull();
    expect(next.gameVersion).toBe(9);
  });

  it("applies the final scores to the players' victory points on game_over", () => {
    const withPlayers = {
      ...state,
      snapshot: {
        view: {
          finished: false,
          players: [
            { id: "seat_a", victory_points: 7 },
            { id: "seat_b", victory_points: 9 },
          ],
        },
      },
    } as unknown as GameSessionState;
    const next = reduceServerMessage(withPlayers, {
      type: "game_over",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      game_version: 9,
      winner: "seat_b",
      final_scores: { seat_a: 8, seat_b: 10 },
    });
    expect(next.snapshot?.view.players.map((p) => p.victory_points)).toEqual([8, 10]);
    expect(next.snapshot?.view.finished).toBe(true);
  });

  it("carries the server's display details into the pending choice", () => {
    const next = reduceServerMessage(state, {
      type: "pending_choice",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      game_version: 7,
      nonce: "n-1",
      choice: {
        player: "seat_a",
        prompt: "spend a strategy token to draw two action cards",
        options: [{ id: "no", label: "decline", kind: "", payload: {} }, { id: "yes", label: "draw", kind: "", payload: {} }],
        details: { kind: "strategy_secondary", card: "pok3politics", tokens_left: 3 },
      },
    } as never);
    expect(next.pendingChoice?.details).toEqual({
      kind: "strategy_secondary",
      card: "pok3politics",
      tokens_left: 3,
    });
  });

  it("leaves details out when the server sent none", () => {
    const next = reduceServerMessage(state, {
      type: "pending_choice",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      game_version: 7,
      nonce: "n-2",
      choice: { player: "seat_a", prompt: "p", options: [{ id: "a", label: "a", kind: "", payload: {} }] },
    } as never);
    expect(next.pendingChoice).not.toHaveProperty("details");
  });

  it("keeps the entire history including early batches", () => {
    const events = Array.from({ length: 510 }, (_, index) => ({
      id: String(index),
      timestamp: "",
      visibility: { visibility: "public" as const },
      event: { kind: "decision_resolved" as const },
      decision_count: index + 1,
      batch_id: index >= 5 && index < 20 ? "basket" : undefined,
    }));
    const visible = serverEventLog(events);
    expect(visible[0].decision_count).toBe(1);
    expect(visible).toHaveLength(510);
  });
  it("refuses malformed history cursors before they reach the UI", () => {
    expect(() =>
      decodeServerMessage(
        { ...snapshot, history: { cursor: -1, redo_count: 0 } },
        "game_12345",
      ),
    ).toThrow(/history status/);
    expect(() =>
      decodeServerMessage(
        {
          type: "event",
          protocol_version: 4,
          game_id: "game_12345",
          entry: { id: "bad", decision_count: 1.5 },
        },
        "game_12345",
      ),
    ).toThrow(/event cursor/);
  });
  it("replaces events and cursor on a newer rewind snapshot, then ignores stale old events", () => {
    const before = reduceServerMessage(
      state,
      decodeServerMessage(
        {
          ...snapshot,
          game_version: 12,
          history: { cursor: 2, redo_count: 0, generation: 0 },
          events: [
            {
              id: "one",
              timestamp: "",
              visibility: { visibility: "public" as const },
              decision_count: 1,
              event: { kind: "decision_resolved" },
            },
            {
              id: "two",
              timestamp: "",
              visibility: { visibility: "public" as const },
              decision_count: 2,
              event: { kind: "decision_resolved" },
            },
          ],
        },
        "game_12345",
      ),
    );
    const after = reduceServerMessage(
      before,
      decodeServerMessage(
        {
          ...snapshot,
          game_version: 13,
          history: { cursor: 1, redo_count: 1, generation: 1 },
          events: [before.events[0]],
        },
        "game_12345",
      ),
    );
    const late = reduceServerMessage(
      after,
      decodeServerMessage(
        {
          type: "event",
          protocol_version: 4,
          game_id: "game_12345",
          entry: { ...before.events[1], version: 12 },
        },
        "game_12345",
      ),
    );
    expect(after.events.map((event) => event.id)).toEqual(["one"]);
    expect(after.history).toMatchObject({
      cursor: 1,
      redo_count: 1,
      generation: 1,
    });
    expect(late).toBe(after);
  });
  it("uses one snapshot reducer and refuses older state-bearing messages", () => {
    const current = reduceServerMessage(
      state,
      decodeServerMessage(snapshot, "game_12345"),
    );
    const stale = reduceServerMessage(
      current,
      decodeServerMessage(
        {
          ...snapshot,
          type: "state_update",
          game_version: 3,
          view: { ...snapshot.view, round: 99 },
        },
        "game_12345",
      ),
    );

    expect(current.snapshot?.view.round).toBe(1);
    expect(stale).toBe(current);
  });

  it("replaces the projection with an accepted state update instead of merging stale snapshot fields", () => {
    const current = reduceServerMessage(
      state,
      decodeServerMessage(snapshot, "game_12345"),
    );
    const update = reduceServerMessage(
      current,
      decodeServerMessage(
        {
          ...snapshot,
          type: "state_update",
          game_version: 5,
          view: { ...snapshot.view, active_player: "seat_a" },
          pending_choice: {
            prompt: "Choose",
            actor: "seat_a",
            nonce: "nonce-5",
            options: [],
          },
        },
        "game_12345",
      ),
    );

    expect(update.gameVersion).toBe(5);
    expect(update.snapshot?.type).toBe("state_update");
    expect(update.pendingChoice?.nonce).toBe("nonce-5");
  });
});

describe("lobby decoding", () => {
  it("mirrors the server byte bound and rejects whitespace, controls, and format characters", () => {
    for (const name of [
      "Z",
      "A".repeat(64),
      "🪐".repeat(16),
      "Ana María",
      "Same",
    ])
      expect(validNickname(name)).toBe(true);
    for (const name of [
      "",
      " ",
      " x",
      "x ",
      "🪐".repeat(17),
      "x\n",
      "x\u202e",
      "x\u200b",
      "x\u{e0001}",
    ])
      expect(validNickname(name)).toBe(false);
  });
  it("accepts the actual server-generated 256-bit player ID and session credential on create and join", () => {
    const playerId = `player_${"a".repeat(64)}`;
    const playerSession = `session_${"b".repeat(64)}`;
    const lobby = {
      game_id: "game_12345",
      phase: "lobby",
      lobby_version: 1,
      host_player_id: playerId,
      slots: [
        {
          slot_id: "slot_1",
          position: 1,
          occupant: playerId,
          nickname: "Host",
          ready: false,
          connected: false,
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
    const created = decodeCreateGameResponse({
      game_id: "game_12345",
      player_session: playerSession,
      player: { id: playerId },
      lobby,
    });
    expect(created.player_session).toBe(playerSession);
    expect(created.player.id).toBe(playerId);
    expect(
      decodeJoinResponse(
        { player_session: playerSession, player: { id: playerId }, lobby },
        "game_12345",
      ).player_session,
    ).toBe(playerSession);
    expect(() =>
      decodeCreateGameResponse({ ...created, player_session: "x".repeat(129) }),
    ).toThrow(/invalid game creation response/);
  });

  it("decodes public slots without a credential or viewer identity", () => {
    const lobby = decodeLobby(
      {
        game_id: "game_12345",
        phase: "running",
        lobby_version: 1,
        host_player_id: "player_a",
        slots: [
          {
            slot_id: "slot_1",
            position: 1,
            occupant: "player_a",
            nickname: "Same",
            ready: true,
            connected: false,
            can_take_over: true,
          },
          {
            slot_id: "slot_2",
            position: 2,
            occupant: "player_b",
            nickname: "Same",
            ready: true,
            connected: true,
            can_take_over: false,
          },
        ],
      },
      "game_12345",
    );
    expect(lobby.slots[0].occupant).toBe("player_a");
    expect(lobby.slots.map((slot) => slot.nickname)).toEqual(["Same", "Same"]);
    expect(JSON.stringify(lobby)).not.toContain("player_session");
    expect(() =>
      decodeLobby(
        {
          ...lobby,
          slots: [{ ...lobby.slots[0], nickname: "x\u202e" }, lobby.slots[1]],
        },
        "game_12345",
      ),
    ).toThrow(/invalid lobby slot/);
  });
});

class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static latest: FakeWebSocket | null = null;
  readyState = FakeWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  sent: string[] = [];

  constructor(_url: string) {
    FakeWebSocket.latest = this;
  }

  send(value: string): void {
    this.sent.push(value);
  }

  close(): void {
    this.readyState = 3;
    this.onclose?.();
  }
}

describe("GameSessionClient ingress lifecycle", () => {
  it("retries an uncertain basket confirmation with the same request ID", async () => {
    const { client, send } = await connectedPlayer();
    send({
      ...snapshot,
      type: "initial_snapshot",
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce: "nonce-4",
        choice: {
          player: "player_a",
          prompt: "Pay",
          context: { subtype: "pay_resources" },
          options: [{ id: "trade_good", kind: "pay", label: "Trade good", payload: {} }],
        },
      },
    });
    const request = vi
      .fn()
      .mockRejectedValueOnce(new Error("Connection lost"))
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          active: true,
          snapshot: {
            ...snapshot,
            game_version: 5,
            viewer: { role: "player", seat: "player_a" },
          },
        }),
      });
    vi.stubGlobal("fetch", request);
    const plan = {
      kind: "payment" as const,
      steps: [{ kind: "trade_good" as const }],
    };
    await expect(client.submitBatch(plan)).rejects.toThrow("Connection lost");
    await client.submitBatch(plan);
    const first = JSON.parse(request.mock.calls[0][1].body);
    const second = JSON.parse(request.mock.calls[1][1].body);
    expect(first).toMatchObject({
      plan,
      expected_version: 4,
      nonce: "nonce-4",
    });
    expect(second.request_id).toBe(first.request_id);
    expect(request).toHaveBeenCalledTimes(2);
    client.stop();
  });

  describe("one confirmation per decision (stale-batch 409)", () => {
    const payDecision = (nonce: string, version: number) => ({
      ...snapshot,
      type: "initial_snapshot" as const,
      game_version: version,
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce,
        choice: {
          player: "player_a",
          prompt: "pay 1 more resources",
          context: { subtype: "pay_resources" },
          options: [{ id: "trade_good", kind: "pay", label: "Trade good", payload: {} }],
        },
      },
    });
    const pay = { kind: "payment" as const, steps: [{ kind: "trade_good" as const }] };
    /** A fetch whose answer the test releases by hand (the batch commit is slow). */
    const slowServer = (answerVersion: number) => {
      let release!: () => void;
      const gate = new Promise<void>((done) => {
        release = done;
      });
      const request = vi.fn().mockImplementation(async () => {
        await gate;
        return {
          ok: true,
          json: async () => ({
            active: true,
            snapshot: { ...snapshot, game_version: answerVersion, viewer: { role: "player", seat: "player_a" } },
          }),
        };
      });
      vi.stubGlobal("fetch", request);
      return { request, release };
    };

    it("a 409 stale decision boundary already overtaken by a newer accepted version is quiet", async () => {
      const { client, send } = await connectedPlayer();
      send(payDecision("nonce-pay", 337));
      let refuse!: () => void;
      const gate = new Promise<void>((done) => (refuse = done));
      vi.stubGlobal(
        "fetch",
        vi.fn().mockImplementation(async () => {
          await gate;
          return {
            ok: false,
            status: 409,
            text: async () => "stale decision boundary: version 337 is behind; refresh and plan again",
          };
        }),
      );
      const manual = client.submitBatch(pay);
      // The answer that won arrives over the socket first.
      send({ ...snapshot, type: "initial_snapshot" as const, game_version: 339, viewer: { role: "player", seat: "player_a" } });
      refuse();
      await expect(manual).resolves.toBeUndefined();
      client.stop();
    });

    it("a 409 stale decision boundary nothing has overtaken still reports", async () => {
      const { client, send } = await connectedPlayer();
      send(payDecision("nonce-pay", 337));
      vi.stubGlobal(
        "fetch",
        vi.fn().mockResolvedValue({
          ok: false,
          status: 409,
          text: async () => "stale decision boundary: x; refresh and plan again",
        }),
      );
      await expect(client.submitBatch(pay)).rejects.toThrow(/stale decision boundary/);
      client.stop();
    });

    it("two confirmations of the same plan while the request runs send one request", async () => {
      const { client, send } = await connectedPlayer();
      send(payDecision("nonce-pay", 337));
      const { request, release } = slowServer(339);
      const first = client.submitBatch(pay);
      const second = client.submitBatch(pay);
      release();
      await Promise.all([first, second]);
      expect(request).toHaveBeenCalledTimes(1);
      expect(JSON.parse(request.mock.calls[0][1].body)).toMatchObject({ expected_version: 337, nonce: "nonce-pay" });
      client.stop();
    });

    it("a different plan for the same decision is refused while one is running (no stale second request)", async () => {
      const { client, send } = await connectedPlayer();
      send(payDecision("nonce-pay", 337));
      const { request, release } = slowServer(339);
      const prepared = client.submitBatch(pay);
      const manual = client.submitBatch({ kind: "payment", steps: [{ kind: "exhaust", planet: "archonren" }] });
      await expect(manual).rejects.toThrow(/still being sent/);
      release();
      await prepared;
      expect(request).toHaveBeenCalledTimes(1);
      client.stop();
    });

    it("after the answer was applied a late repeat finds no open decision and sends nothing", async () => {
      const { client, send } = await connectedPlayer();
      send(payDecision("nonce-pay", 337));
      const { request, release } = slowServer(339);
      const first = client.submitBatch(pay);
      release();
      await first;
      await expect(client.submitBatch(pay)).rejects.toThrow(/no longer pending/);
      expect(request).toHaveBeenCalledTimes(1);
      client.stop();
    });

    it("a failed request frees the decision for a retry", async () => {
      const { client, send } = await connectedPlayer();
      send(payDecision("nonce-pay", 337));
      const request = vi
        .fn()
        .mockRejectedValueOnce(new Error("Connection lost"))
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({
            active: true,
            snapshot: { ...snapshot, game_version: 339, viewer: { role: "player", seat: "player_a" } },
          }),
        });
      vi.stubGlobal("fetch", request);
      await expect(client.submitBatch(pay)).rejects.toThrow("Connection lost");
      await client.submitBatch(pay);
      expect(request).toHaveBeenCalledTimes(2);
      client.stop();
    });

    it("a payment after a production batch is built on the fresh version and nonce", async () => {
      const { client, send } = await connectedPlayer();
      send({
        ...payDecision("nonce-produce", 335),
        pending_choice: {
          nonce: "nonce-produce",
          choice: {
            player: "player_a",
            prompt: "produce in 14 (4 left)",
            context: { subtype: "produce_unit" },
            options: [{ id: "build|fighter|2", kind: "produce", label: "produce 2x fighter for 1", payload: {} }],
          },
        },
      });
      const answer = (version: number, decision: object) => ({
        ok: true,
        json: async () => ({
          active: true,
          snapshot: { ...payDecision("nonce-pay", version), ...decision },
        }),
      });
      const request = vi.fn().mockResolvedValueOnce(answer(337, {})).mockResolvedValueOnce(answer(339, { pending_choice: null }));
      vi.stubGlobal("fetch", request);
      await client.submitBatch({
        kind: "production",
        destination: "14",
        steps: [{ kind: "produce", unit: "fighter", count: 2 }],
      });
      expect(client.getState().pendingChoice?.nonce).toBe("nonce-pay");
      await client.submitBatch(pay);
      const bodies = request.mock.calls.map((call) => JSON.parse(call[1].body));
      expect(bodies[0]).toMatchObject({ expected_version: 335, nonce: "nonce-produce" });
      expect(bodies[1]).toMatchObject({ expected_version: 337, nonce: "nonce-pay" });
      expect(bodies[1].request_id).not.toBe(bodies[0].request_id);
      client.stop();
    });
  });

  it("sends a token plan while a command token gain is pending, and refuses it otherwise", async () => {
    const { client, send } = await connectedPlayer();
    const pending = (subtype: string) => ({
      ...snapshot,
      type: "initial_snapshot" as const,
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce: "nonce-5",
        choice: {
          player: "player_a",
          prompt: "gain a command token into which pool",
          context: { subtype },
          options: [{ id: "tactic_tokens", kind: "pool", label: "tactic pool", payload: {} }],
        },
      },
    });
    const plan = {
      kind: "tokens" as const,
      steps: [{ kind: "pool" as const, pool: "tactic_tokens" }],
    };
    send(pending("ready_planet"));
    await expect(client.submitBatch(plan)).rejects.toThrow("Workflow is no longer pending");
    send(pending("gain_command_token"));
    const request = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        active: true,
        snapshot: { ...snapshot, game_version: 5, viewer: { role: "player", seat: "player_a" } },
      }),
    });
    vi.stubGlobal("fetch", request);
    await client.submitBatch(plan);
    expect(JSON.parse(request.mock.calls[0][1].body)).toMatchObject({ plan, nonce: "nonce-5" });
    client.stop();
  });

  it("sends a ready_planets plan only while a ready question is pending", async () => {
    const { client, send } = await connectedPlayer();
    const pending = (subtype: string) => ({
      ...snapshot,
      type: "initial_snapshot" as const,
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce: "nonce-9",
        choice: {
          player: "player_a",
          prompt: "ready which planet",
          context: { subtype },
          options: [
            { id: "jord", kind: "ready", label: "ready jord", payload: {} },
            { id: "lodor", kind: "ready", label: "ready lodor", payload: {} },
          ],
        },
      },
    });
    const plan = {
      kind: "ready_planets" as const,
      steps: [
        { kind: "ready_planet" as const, planet: "jord" },
        { kind: "ready_planet" as const, planet: "lodor" },
      ],
    };
    send(pending("gain_command_token"));
    await expect(client.submitBatch(plan)).rejects.toThrow("Workflow is no longer pending");
    send(pending("ready_planet"));
    const request = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        active: true,
        snapshot: { ...snapshot, game_version: 5, viewer: { role: "player", seat: "player_a" } },
      }),
    });
    vi.stubGlobal("fetch", request);
    await client.submitBatch(plan);
    expect(JSON.parse(request.mock.calls[0][1].body)).toMatchObject({ plan, nonce: "nonce-9" });
    client.stop();
  });

  describe("a plan paused at a reaction window", () => {
    let version = 6;
    const pendingAt = (subtype: string, nonce: string) => ({
      ...snapshot,
      type: "initial_snapshot" as const,
      game_version: version++,
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce,
        choice: {
          player: "player_a",
          prompt: subtype,
          context: { subtype },
          options: [{ id: "decline", kind: "decline", label: "decline", payload: {} }],
        },
      },
    });
    const plan = {
      kind: "agenda_vote_planets" as const,
      steps: [
        { kind: "vote_planet" as const, planet: "jord" },
        { kind: "vote_planet" as const, planet: "arc_prime" },
        { kind: "done_voting" as const },
      ],
    };
    // A confirmed batch reconnects the client, so later server messages arrive on the new socket.
    const later = (message: object) => {
      const socket = FakeWebSocket.latest!;
      socket.readyState = FakeWebSocket.OPEN;
      socket.onmessage?.({
        data: JSON.stringify({
          protocol_version: PROTOCOL_VERSION,
          game_id: "game_12345",
          ...message,
        }),
      } as MessageEvent);
    };
    const paused = (remaining: unknown[], shot: unknown) => ({
      ok: true,
      json: async () => ({
        active: true,
        interrupted: {
          applied_steps: 1,
          remaining_steps: remaining,
          offered: { subtype: "reaction_after_VOTES_CAST", own_seat: true },
        },
        snapshot: shot,
      }),
    });

    it("resolves without error, keeps the remainder, and sends it again on request", async () => {
      const { client, send } = await connectedPlayer();
      send(pendingAt("vote_exhaust_planet", "nonce-v1"));
      const remaining = plan.steps.slice(1);
      const request = vi
        .fn()
        .mockResolvedValueOnce(
          paused(remaining, pendingAt("reaction_after_VOTES_CAST", "nonce-r")),
        )
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({ active: true, snapshot: pendingAt("agenda_vote", "nonce-done") }),
        });
      vi.stubGlobal("fetch", request);
      await expect(client.submitBatch(plan)).resolves.toBeUndefined();
      expect(client.getState().batchResume).toMatchObject({
        applied: 1,
        plan: { kind: "agenda_vote_planets", steps: remaining },
        waiting: { subtype: "reaction_after_VOTES_CAST", ownSeat: true },
      });
      expect(client.getState().lastError).toBeNull();
      // The reaction is pending: the plan cannot be continued yet, the server would call it stale.
      await expect(client.resumeBatch()).rejects.toThrow("Workflow is no longer pending");
      later(pendingAt("vote_exhaust_planet", "nonce-v2"));
      expect(client.getState().batchResume).not.toBeNull();
      await client.resumeBatch();
      const body = JSON.parse(request.mock.calls[1][1].body);
      expect(body.plan).toEqual({ kind: "agenda_vote_planets", steps: remaining });
      expect(body.nonce).toBe("nonce-v2");
      expect(client.getState().batchResume).toBeNull();
      client.stop();
    });

    it("drops the remainder when the server refuses it, and when the game moves on", async () => {
      const { client, send } = await connectedPlayer();
      send(pendingAt("vote_exhaust_planet", "nonce-v1"));
      const remaining = plan.steps.slice(1);
      vi.stubGlobal(
        "fetch",
        vi
          .fn()
          .mockResolvedValueOnce(paused(remaining, pendingAt("vote_exhaust_planet", "nonce-v2")))
          .mockResolvedValueOnce({
            ok: false,
            status: 409,
            text: async () => JSON.stringify({ message: "option unavailable" }),
          }),
      );
      await client.submitBatch(plan);
      expect(client.getState().batchResume).not.toBeNull();
      await expect(client.resumeBatch()).rejects.toThrow("option unavailable");
      expect(client.getState().batchResume).toBeNull();
      expect(client.getState().lastError).toContain("option unavailable");

      vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(paused(remaining, pendingAt("reaction_after_VOTES_CAST", "r"))));
      later(pendingAt("vote_exhaust_planet", "nonce-v3"));
      await client.submitBatch(plan);
      expect(client.getState().batchResume).not.toBeNull();
      later(pendingAt("action_phase", "nonce-a"));
      expect(client.getState().batchResume).toBeNull();
      client.stop();
    });

    it("treats a plan the server applied whole as finished", async () => {
      const { client, send } = await connectedPlayer();
      send(pendingAt("vote_exhaust_planet", "nonce-v1"));
      vi.stubGlobal(
        "fetch",
        vi.fn().mockResolvedValue({
          ok: true,
          json: async () => ({ active: true, snapshot: pendingAt("agenda_vote", "n2") }),
        }),
      );
      await client.submitBatch(plan);
      expect(client.getState().batchResume).toBeNull();
      client.stop();
    });
  });

  it("sends a casualty plan while a sustain or casualty decision is pending", async () => {
    const { client, send } = await connectedPlayer();
    send({
      ...snapshot,
      type: "initial_snapshot",
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce: "nonce-4",
        choice: {
          player: "player_a",
          prompt: "cancel a hit at 18",
          context: { subtype: "sustain_damage" },
          options: [{ id: "decline", kind: "decline", label: "take the hit", payload: {} }],
        },
      },
    });
    const request = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        active: true,
        snapshot: {
          ...snapshot,
          game_version: 5,
          viewer: { role: "player", seat: "player_a" },
        },
      }),
    });
    vi.stubGlobal("fetch", request);
    const plan = {
      kind: "casualties" as const,
      steps: [{ kind: "destroy" as const, unit: "fighter", damaged: false }],
    };
    await client.submitBatch(plan);
    expect(JSON.parse(request.mock.calls[0][1].body)).toMatchObject({
      plan,
      nonce: "nonce-4",
    });
    client.stop();
  });

  it("refreshes the version after an in-flight history conflict without changing the undo target", async () => {
    const { client } = await connectedPlayer();
    const request = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 409,
        text: async () => "Game advanced or a decision is in flight",
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          ...snapshot,
          game_version: 6,
          viewer: { role: "player", seat: "player_a" },
          history: { cursor: 0, redo_count: 0 },
        }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          ...snapshot,
          game_version: 7,
          viewer: { role: "player", seat: "player_a" },
          history: { cursor: 0, redo_count: 1, generation: 1 },
          events: [],
        }),
      });
    vi.stubGlobal("fetch", request);
    await client.changeHistory("undo_pipeline");
    expect(JSON.parse(request.mock.calls[0][1].body)).toEqual({
      action: "undo_pipeline",
      expected_version: 4,
    });
    expect(request.mock.calls[1][0]).toBe("/api/games/game_12345/snapshot");
    expect(JSON.parse(request.mock.calls[2][1].body)).toEqual({
      action: "undo_pipeline",
      expected_version: 6,
    });
    client.stop();
  });

  it("fetches the replay with the player session and names the file after the game", async () => {
    const { client } = await connectedPlayer();
    const request = vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({ format: "ti4-replay", history: { decisions: [] } }),
    });
    vi.stubGlobal("fetch", request);
    const replay = await client.fetchReplay();
    expect(request.mock.calls[0][0]).toBe("/api/games/game_12345/replay");
    expect(request.mock.calls[0][1].headers).toHaveProperty("x-ti4-player-session");
    expect(replay.filename).toBe("ti4-replay-game_12345.json");
    expect(JSON.parse(replay.text)).toEqual({ format: "ti4-replay", history: { decisions: [] } });
    client.stop();
  });

  it("surfaces the server's reason when the replay is refused", async () => {
    const { client } = await connectedPlayer();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 403,
        text: async () => "the session credential is not valid for this game",
      }),
    );
    await expect(client.fetchReplay()).rejects.toThrow(/session credential is not valid/);
    client.stop();
  });

  it("reads the turn redo status with the player session and decodes it", async () => {
    const { client } = await connectedPlayer();
    const request = vi
      .fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ status: null }) })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          status: {
            seat: "player_a",
            requested_by: "player_a",
            turns_back: 1,
            redo_count: 1,
            stage: "new_turn",
            original_decisions: 10,
            rewound_to: 4,
            turn_complete: false,
            handoff_len: null,
            outcome: null,
            can_control: true,
          },
        }),
      });
    vi.stubGlobal("fetch", request);
    expect(await client.fetchTurnRedoStatus()).toBeNull();
    expect(request.mock.calls[0][0]).toBe("/api/games/game_12345/turn-redo");
    expect(request.mock.calls[0][1].headers).toHaveProperty("x-ti4-player-session");
    expect((await client.fetchTurnRedoStatus())?.stage).toBe("new_turn");
    client.stop();
  });

  it("posts a turn redo request and switches to the replacement timeline", async () => {
    const { client } = await connectedPlayer();
    const request = vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ...snapshot,
        game_version: 9,
        viewer: { role: "player", seat: "player_a" },
        history: { cursor: 2, redo_count: 0, generation: 3 },
        events: [],
      }),
    });
    vi.stubGlobal("fetch", request);
    await client.turnRedoCommand({ action: "request", turns: 2 });
    expect(request.mock.calls[0][0]).toBe("/api/games/game_12345/turn-redo");
    expect(JSON.parse(request.mock.calls[0][1].body)).toEqual({
      action: "request",
      turns: 2,
      expected_version: 4,
    });
    expect(client.getState().gameVersion).toBe(9);
    expect(client.getState().history.generation).toBe(3);
    client.stop();
  });

  it("surfaces a refused turn redo and keeps the timeline", async () => {
    const { client } = await connectedPlayer();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValueOnce({
        ok: false,
        status: 403,
        text: async () => "History change forbidden: only the host may redo another seat's turn",
      }),
    );
    await expect(client.turnRedoCommand({ action: "request", seat: "player_b" })).rejects.toThrow(
      /Turn redo failed \(403\).*only the host/,
    );
    expect(client.getState().gameVersion).toBe(4);
    client.stop();
  });

  it("leaves the live connection and an answer in flight alone when an auto-play replaced nothing", async () => {
    const { client, socket, send } = await connectedPlayer();
    const pending = {
      nonce: "nonce-4",
      choice: { player: "player_a", prompt: "Choose", options: [{ id: "opt-4", label: "Choose", kind: "", payload: {} }] },
    };
    send({
      ...snapshot,
      type: "initial_snapshot",
      viewer: { role: "player", seat: "player_a" },
      history: { cursor: 3, redo_count: 0, generation: 3 },
      pending_choice: pending,
    });
    expect(client.getState().history.generation).toBe(3);
    // The player answers while the redoing tab asks for the auto-play of a turn redone mid-turn:
    // nothing was recorded after it, so the server answers with the live game (same generation).
    let settled: string | null = null;
    const submitted = client.submitChoice("opt-4").then(
      () => (settled = "accepted"),
      (error: Error) => (settled = error.message),
    );
    const request = vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ...snapshot,
        game_version: 4,
        viewer: { role: "player", seat: "player_a" },
        history: { cursor: 3, redo_count: 0, generation: 3 },
        pending_choice: pending,
      }),
    });
    vi.stubGlobal("fetch", request);
    await client.turnRedoCommand({ action: "autoplay" });
    expect(FakeWebSocket.latest).toBe(socket);
    expect(socket.readyState).toBe(FakeWebSocket.OPEN);
    expect(client.getState().lastError).toBeNull();
    // The answer still goes through on the same socket.
    send({ type: "action_accepted", game_version: 4, option_id: "opt-4" });
    send({
      ...snapshot,
      type: "state_update",
      game_version: 5,
      viewer: { role: "player", seat: "player_a" },
      pending_choice: null,
    });
    await submitted;
    expect(settled).toBe("accepted");
    client.stop();
  });

  it("keeps the new timeline without replacing the session", async () => {
    const { client } = await connectedPlayer();
    const request = vi.fn().mockResolvedValueOnce({ ok: true, json: async () => ({ kept: true }) });
    vi.stubGlobal("fetch", request);
    await client.turnRedoCommand({ action: "keep" });
    expect(JSON.parse(request.mock.calls[0][1].body)).toEqual({ action: "keep", expected_version: 4 });
    expect(client.getState().gameVersion).toBe(4);
    client.stop();
  });

  it("keeps retrying with the fresh version when bots moved the history cursor meanwhile", async () => {
    const { client } = await connectedPlayer();
    const request = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 409,
        text: async () => "Game advanced or a decision is in flight: the request expected version 4",
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          ...snapshot,
          game_version: 6,
          viewer: { role: "player", seat: "player_a" },
          history: { cursor: 3, redo_count: 0 },
        }),
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 409,
        text: async () => "Game advanced during rewind",
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          ...snapshot,
          game_version: 8,
          viewer: { role: "player", seat: "player_a" },
          history: { cursor: 5, redo_count: 0 },
        }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          ...snapshot,
          game_version: 9,
          viewer: { role: "player", seat: "player_a" },
          history: { cursor: 2, redo_count: 3, generation: 1 },
          events: [],
        }),
      });
    vi.stubGlobal("fetch", request);
    await client.changeHistory("undo");
    expect(request).toHaveBeenCalledTimes(5);
    expect(JSON.parse(request.mock.calls[4][1].body)).toEqual({
      action: "undo",
      expected_version: 8,
    });
    client.stop();
  });

  it("says the game moved on, not the raw 409, when the retries run out", async () => {
    const { client } = await connectedPlayer();
    const request = vi.fn().mockImplementation(async (url: string) =>
      url.endsWith("/snapshot")
        ? {
            ok: true,
            json: async () => ({
              ...snapshot,
              game_version: 6,
              viewer: { role: "player", seat: "player_a" },
              history: { cursor: 1, redo_count: 0 },
            }),
          }
        : { ok: false, status: 409, text: async () => "Game advanced during rewind" },
    );
    vi.stubGlobal("fetch", request);
    vi.useFakeTimers();
    const outcome = expect(client.changeHistory("redo")).rejects.toThrow(
      "The game moved on while redoing — try again",
    );
    await vi.runAllTimersAsync();
    await outcome;
    vi.useRealTimers();
    client.stop();
  });

  it("recognizes both server conflict phrases and words the failure plainly", () => {
    expect(isHistoryVersionConflict("Game advanced or a decision is in flight: history is still reloading")).toBe(true);
    expect(isHistoryVersionConflict("Game advanced during rewind")).toBe(true);
    expect(isHistoryVersionConflict("there is nothing to undo")).toBe(false);
    expect(historyFailureMessage("History change", 409, "Game advanced during rewind", "undoing")).toBe(
      "The game moved on while undoing — try again",
    );
    expect(historyFailureMessage("History change", 403, "only the host")).toBe(
      "History change failed (403): only the host",
    );
  });

  it("posts host rewind with the current version, drops pending submissions and reconnects", async () => {
    const { client, socket } = await connectedPlayer();
    const submitted = client.submitChoice("opt-4");
    const rejected = expect(submitted).rejects.toThrow(/history changed/i);
    const request = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        ...snapshot,
        game_version: 5,
        viewer: { role: "player", seat: "player_a" },
        history: { cursor: 0, redo_count: 1, generation: 1 },
        events: [],
        pending_choice: null,
      }),
    });
    vi.stubGlobal("fetch", request);
    await client.changeHistory("undo");
    await rejected;
    expect(request).toHaveBeenCalledWith(
      "/api/games/game_12345/history",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ action: "undo", expected_version: 4 }),
      }),
    );
    expect(client.getState().history.redo_count).toBe(1);
    expect(client.getState().pendingChoice).toBeNull();
    expect(socket.readyState).toBe(3);
    expect(FakeWebSocket.latest).not.toBe(socket);
    client.stop();
  });
  it("posts a log row's actual decision cursor rather than its visible row index", async () => {
    const { client } = await connectedPlayer();
    const request = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        ...snapshot,
        game_version: 5,
        viewer: { role: "player", seat: "player_a" },
        history: { cursor: 2, redo_count: 2, generation: 1 },
        events: [],
      }),
    });
    vi.stubGlobal("fetch", request);
    await client.changeHistory({ cursor: 2 });
    expect(JSON.parse(request.mock.calls[0][1].body)).toEqual({
      action: "restore_cursor",
      cursor: 2,
      expected_version: 4,
    });
    client.stop();
  });
  it("sends set_reaction_mode and takes the modes from the seat's next state update", async () => {
    const { client, socket, send } = await connectedPlayer();
    expect(client.getState().snapshot?.reaction_modes).toBeUndefined();
    client.setReactionMode("Sabotage", "never");
    expect(JSON.parse(socket.sent.at(-1)!)).toEqual({
      type: "set_reaction_mode",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      card: "Sabotage",
      mode: "never",
    });
    // Nothing is assumed locally: the modes are what the server last said.
    expect(client.getState().snapshot?.reaction_modes).toBeUndefined();
    send({
      ...snapshot,
      type: "state_update",
      game_version: 4,
      viewer: { role: "player", seat: "player_a" },
      reaction_modes: { Sabotage: "never", Junk: "sometimes" },
    });
    expect(client.getState().snapshot?.reaction_modes).toEqual({ Sabotage: "never" });
    // A later update without the field is the seat having none.
    send({
      ...snapshot,
      type: "state_update",
      game_version: 5,
      viewer: { role: "player", seat: "player_a" },
    });
    expect(client.getState().snapshot?.reaction_modes).toBeUndefined();
    client.stop();
  });

  it("sends the declared bluff triggers, keeps them per seat, and takes the server's answer", async () => {
    localStorage.clear();
    const { client, socket, send } = await connectedPlayer();
    client.setReactionIntent(["agenda"]);
    expect(JSON.parse(socket.sent.at(-1)!)).toEqual({
      type: "set_reaction_intent",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      triggers: ["agenda"],
    });
    expect(localStorage.getItem("bluff_triggers:game_12345:player_a")).toBe('["agenda"]');
    // The answer carries no game version, so it is never dropped as stale.
    send({
      type: "reaction_intent_state",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      triggers: ["agenda"],
      max_triggers: 3,
      eligible: true,
      budget_used_up: false,
      holding: true,
      locked_until_round: 2,
    });
    expect(client.getState().reactionIntent?.holding).toBe(true);
    // A refusal that comes back with the old declaration puts the stored copy right.
    send({
      type: "reaction_intent_state",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      triggers: [],
      max_triggers: 3,
      eligible: false,
      budget_used_up: false,
      holding: false,
    });
    expect(localStorage.getItem("bluff_triggers:game_12345:player_a")).toBeNull();
    client.passReactionHold();
    expect(JSON.parse(socket.sent.at(-1)!)).toEqual({
      type: "pass_reaction_hold",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
    });
    client.stop();
  });

  it("previews a secondary read-only: the request names the card and answers, the reply resolves the call", async () => {
    const { client, socket, send } = await connectedPlayer();
    const asked = client.previewSecondary("pok7technology", "player_b", ["yes"]);
    const sent = JSON.parse(socket.sent.at(-1)!);
    expect(sent).toEqual({
      type: "preview_secondary",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      request_id: sent.request_id,
      card: "pok7technology",
      primary: "player_b",
      answers: ["yes"],
    });
    send({
      type: "secondary_preview",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      request_id: sent.request_id,
      as_of_version: 7,
      as_of_decisions: 12,
      card: "pok7technology",
      outcome: {
        result: "preview",
        preview: { status: "would_not_be_asked", blocker: "cannot_pay_resources" },
      },
    });
    await expect(asked).resolves.toEqual({
      kind: "preview",
      body: { status: "would_not_be_asked", blocker: "cannot_pay_resources" },
      asOfVersion: 7,
      asOfDecisions: 12,
    });
    // The reply is not part of the shared projection: no error, no pending choice, no version change.
    expect(client.getState().lastError).toBeNull();
    // A refusal resolves too (the caller falls back to its estimate).
    const refused = client.previewSecondary("pok7technology", "player_b", []);
    const second = JSON.parse(socket.sent.at(-1)!);
    send({
      type: "secondary_preview",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      request_id: second.request_id,
      as_of_version: 7,
      as_of_decisions: 0,
      card: "pok7technology",
      outcome: { result: "refused", reason: "already_asked", detail: "you have already been asked" },
    });
    await expect(refused).resolves.toMatchObject({ kind: "refused", reason: "already_asked" });
    client.stop();
  });

  it("treats an older server's unknown-message error as 'no preview' and never shows it", async () => {
    const { client, socket, send } = await connectedPlayer();
    const asked = client.previewSecondary("pok7technology", "player_b", []);
    expect(socket.sent.at(-1)).toContain("preview_secondary");
    send({
      type: "error",
      protocol_version: PROTOCOL_VERSION,
      kind: "malformed_message",
      message: "unknown variant `preview_secondary`, expected one of `subscribe`, `submit_choice`",
    });
    await expect(asked).rejects.toBeInstanceOf(PreviewUnsupportedError);
    expect(client.getState().lastError).toBeNull();
    // Remembered: nothing more is sent to that server.
    const sentBefore = socket.sent.length;
    await expect(client.previewSecondary("pok7technology", "player_b", [])).rejects.toBeInstanceOf(
      PreviewUnsupportedError,
    );
    expect(socket.sent.length).toBe(sentBefore);
    client.stop();
  });

  it("asks who can go to a system read-only and resolves with the engine's answer", async () => {
    const { client, socket, send } = await connectedPlayer();
    const asked = client.queryReach("18");
    const sent = JSON.parse(socket.sent.at(-1)!);
    expect(sent).toEqual({
      type: "query_reach",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      request_id: sent.request_id,
      system_id: "18",
    });
    const origins = [
      {
        owner: "player_b",
        origin: "27",
        ships: [{ type_id: "cruiser", count: 2, damaged: 0, gravity_drive: 1, ionian: 0 }],
      },
    ];
    send({
      type: "reach_into",
      protocol_version: PROTOCOL_VERSION,
      game_id: "game_12345",
      request_id: sent.request_id,
      system_id: "18",
      as_of_version: 5,
      origins,
    });
    await expect(asked).resolves.toEqual({
      kind: "reach",
      systemId: "18",
      asOfVersion: 5,
      origins,
    });
    expect(client.getState().lastError).toBeNull();
    client.stop();
  });

  it("treats an older server's unknown-message error as 'reach unsupported' and never shows it", async () => {
    const { client, socket, send } = await connectedPlayer();
    const asked = client.queryReach("18");
    send({
      type: "error",
      protocol_version: PROTOCOL_VERSION,
      kind: "malformed_message",
      message: "unknown variant `query_reach`, expected one of `subscribe`, `submit_choice`",
    });
    await expect(asked).rejects.toBeInstanceOf(ReachUnsupportedError);
    expect(client.getState().lastError).toBeNull();
    const sentBefore = socket.sent.length;
    await expect(client.queryReach("18")).rejects.toBeInstanceOf(ReachUnsupportedError);
    expect(socket.sent.length).toBe(sentBefore);
    client.stop();
  });

  it("gives up on a preview the server never answers", async () => {
    vi.useFakeTimers();
    try {
      const { client } = await connectedPlayer();
      const asked = client.previewSecondary("pok7technology", "player_b", []);
      const failure = expect(asked).rejects.toThrow(/in time/);
      await vi.advanceTimersByTimeAsync(PREVIEW_TIMEOUT_MS + 10);
      await failure;
      client.stop();
    } finally {
      vi.useRealTimers();
    }
  });

  it("sends a stored declaration again on connect, and nothing when none is stored", async () => {
    localStorage.clear();
    const mine = { ...snapshot, type: "initial_snapshot", viewer: { role: "player", seat: "player_a" } };
    const { client: plain, socket: plainSocket, send: sendPlain } = await connectedPlayer();
    sendPlain(mine);
    expect(plainSocket.sent.some((text) => text.includes("set_reaction_intent"))).toBe(false);
    plain.stop();
    localStorage.setItem("bluff_triggers:game_12345:player_a", '["movement"]');
    const { client, socket, send } = await connectedPlayer();
    send(mine);
    const resent = socket.sent.map((text) => JSON.parse(text)).find((m) => m.type === "set_reaction_intent");
    expect(resent?.triggers).toEqual(["movement"]);
    client.stop();
    localStorage.clear();
  });

  it("does not send a mode change for a spectator or without a connection", () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    const spectator = new GameSessionClient({ gameId: "game_12345", viewer: { role: "spectator" } });
    spectator.setReactionMode("Sabotage", "never");
    expect(spectator.getState().lastError).toMatch(/seated player/);
    const offline = new GameSessionClient({
      gameId: "game_12345",
      viewer: { role: "player", seat: "player_a", playerSession: "private" },
    });
    offline.setReactionMode("Sabotage", "never");
    expect(offline.getState().lastError).toMatch(/not connected/);
  });

  async function connectedPlayer() {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          ...snapshot,
          viewer: { role: "player", seat: "player_a" },
          pending_choice: {
            nonce: "nonce-4",
            choice: {
              player: "player_a",
              prompt: "Choose",
              options: [{ id: "opt-4", label: "Choose", kind: "", payload: {} }],
            },
          },
        }),
      }),
    );
    const client = new GameSessionClient({
      gameId: "game_12345",
      viewer: { role: "player", seat: "player_a", playerSession: "private" },
    });
    client.start();
    const socket = FakeWebSocket.latest!;
    socket.readyState = FakeWebSocket.OPEN;
    socket.onopen?.();
    await vi.waitFor(() =>
      expect(client.getState().pendingChoice?.nonce).toBe("nonce-4"),
    );
    const send = (message: object) =>
      socket.onmessage?.({
        data: JSON.stringify({
          protocol_version: PROTOCOL_VERSION,
          game_id: "game_12345",
          ...message,
        }),
      } as MessageEvent);
    return { client, socket, send };
  }

  it("ignores the answers of the draft protocol", async () => {
    const { client, send } = await connectedPlayer();
    send({ type: "draft_outcome" });
    send({ type: "draft_applied" });
    expect(client.getState().lastError).toBeNull();
    expect(client.getState().pendingChoice?.nonce).toBe("nonce-4");
  });

  it.each([
    { reason: "stale_nonce" },
    { reason: "stale_version", expected: 4, current: 5 },
  ])(
    "rejects a refused submission ($reason) and allows retry",
    async (reason) => {
      const { client, socket, send } = await connectedPlayer();
      const submitted = client.submitChoice("opt-4");
      const refused = expect(submitted).rejects.toThrow(/Rejected:/);
      expect(JSON.parse(socket.sent.at(-1)!)).toMatchObject({
        type: "submit_choice",
        option_id: "opt-4",
        nonce: "nonce-4",
        expected_version: 4,
      });
      send({ type: "action_rejected", game_version: 4, reason });
      await refused;
      expect(client.getState().lastError).toMatch(/Rejected:/);
      const retried = client.submitChoice("opt-4");
      expect(
        socket.sent.filter(
          (message) => JSON.parse(message).type === "submit_choice",
        ),
      ).toHaveLength(2);
      const stopped = expect(retried).rejects.toThrow(/disconnected|stopped/i);
      client.stop();
      await stopped;
    },
  );

  it("abandons a submission that gets no reply so the next click sends a new frame", async () => {
    const { client, socket } = await connectedPlayer();
    vi.useFakeTimers();
    try {
      const first = client.submitChoice("opt-4");
      const timedOut = expect(first).rejects.toThrow(/no response/i);
      await vi.advanceTimersByTimeAsync(10_000);
      await timedOut;
      const second = client.submitChoice("opt-4");
      expect(
        socket.sent.filter(
          (message) => JSON.parse(message).type === "submit_choice",
        ),
      ).toHaveLength(2);
      const stopped = expect(second).rejects.toThrow(/disconnected|stopped/i);
      client.stop();
      await stopped;
    } finally {
      vi.useRealTimers();
    }
  });

  it.each(["ack-first", "update-first"])(
    "waits for acceptance and a newer authoritative state (%s)",
    async (order) => {
      const { client, send } = await connectedPlayer();
      const submitted = client.submitChoice("opt-4");
      let settled = false;
      void submitted.then(() => {
        settled = true;
      });
      const accepted = () =>
        send({ type: "action_accepted", game_version: 4, option_id: "opt-4" });
      const update = () =>
        send({
          ...snapshot,
          type: "state_update",
          game_version: 5,
          viewer: { role: "player", seat: "player_a" },
          pending_choice: {
            nonce: "nonce-5",
            choice: {
              player: "player_a",
              prompt: "Next",
              options: [{ id: "opt-5", label: "Next", kind: "", payload: {} }],
            },
          },
        });
      if (order === "ack-first") accepted();
      else update();
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(settled).toBe(false);
      if (order === "ack-first") update();
      else accepted();
      await submitted;
      expect(client.getState().pendingChoice?.nonce).toBe("nonce-5");
      client.stop();
    },
  );

  it.each(["ack-first", "choice-first"])(
    "releases a confirmed submission when a newer pending choice arrives before its state update (%s)",
    async (order) => {
      const { client, socket, send } = await connectedPlayer();
      const submitted = client.submitChoice("opt-4");
      const accepted = () =>
        send({ type: "action_accepted", game_version: 4, option_id: "opt-4" });
      const nextChoice = () =>
        send({
          type: "pending_choice",
          game_version: 5,
          nonce: "nonce-5",
          choice: {
            player: "player_a",
            prompt: "Sustain damage",
            context: { subtype: "sustain_damage" },
            options: [{ id: "sustain", kind: "sustain", label: "Sustain", payload: {} }],
          },
          state: {},
          galaxy_layout: snapshot.galaxy_layout,
        });
      if (order === "ack-first") accepted();
      else nextChoice();
      if (order === "ack-first") nextChoice();
      else accepted();
      await submitted;
      expect(client.getState().snapshot?.game_version).toBe(4);
      expect(client.getState().pendingChoice?.nonce).toBe("nonce-5");
      const nextSubmission = client.submitChoice("sustain");
      expect(JSON.parse(socket.sent.at(-1)!)).toMatchObject({
        type: "submit_choice",
        nonce: "nonce-5",
        expected_version: 5,
        option_id: "sustain",
      });
      const stopped = expect(nextSubmission).rejects.toThrow(/stopped/i);
      client.stop();
      await stopped;
    },
  );

  it("rejects an in-flight submission on disconnect and permits a fresh submission after reconnect", async () => {
    const { client, socket } = await connectedPlayer();
    vi.useFakeTimers();
    const submitted = client.submitChoice("opt-4");
    const failed = expect(submitted).rejects.toThrow(/disconnected/i);
    socket.onclose?.();
    await failed;
    await expect(client.submitChoice("opt-4")).rejects.toThrow(
      /not connected/i,
    );
    await vi.advanceTimersByTimeAsync(2_000);
    const reconnected = FakeWebSocket.latest!;
    expect(reconnected).not.toBe(socket);
    reconnected.readyState = FakeWebSocket.OPEN;
    reconnected.onopen?.();
    reconnected.onmessage?.({
      data: JSON.stringify({
        ...snapshot,
        viewer: { role: "player", seat: "player_a" },
        pending_choice: {
          nonce: "nonce-4",
          choice: {
            player: "player_a",
            prompt: "Choose",
            options: [{ id: "opt-4", label: "Choose", kind: "", payload: {} }],
          },
        },
      }),
    } as MessageEvent);
    const retried = client.submitChoice("opt-4");
    expect(JSON.parse(reconnected.sent.at(-1)!)).toMatchObject({
      type: "submit_choice",
      nonce: "nonce-4",
    });
    reconnected.onmessage?.({
      data: JSON.stringify({
        type: "action_accepted",
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_12345",
        game_version: 4,
        option_id: "opt-4",
      }),
    } as MessageEvent);
    reconnected.onmessage?.({
      data: JSON.stringify({
        ...snapshot,
        type: "state_update",
        game_version: 5,
        viewer: { role: "player", seat: "player_a" },
        pending_choice: null,
      }),
    } as MessageEvent);
    await retried;
    client.stop();
  });

  it("does not accept a different option or an unchanged choice as progress", async () => {
    const { client, send } = await connectedPlayer();
    const submitted = client.submitChoice("opt-4");
    let settled = false;
    void submitted.then(
      () => {
        settled = true;
      },
      () => {
        settled = true;
      },
    );
    send({ type: "action_accepted", game_version: 4, option_id: "other" });
    send({
      ...snapshot,
      type: "state_update",
      game_version: 5,
      viewer: { role: "player", seat: "player_a" },
      pending_choice: {
        nonce: "nonce-4",
        choice: {
          player: "player_a",
          prompt: "Choose",
          options: [{ id: "opt-4", label: "Choose", kind: "", payload: {} }],
        },
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(settled).toBe(false);
    send({ type: "action_accepted", game_version: 4, option_id: "opt-4" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(settled).toBe(false);
    const failed = expect(submitted).rejects.toThrow(
      /Rejected: Stale decision nonce/,
    );
    send({
      type: "action_rejected",
      game_version: 4,
      reason: { reason: "stale_nonce" },
    });
    await failed;
    expect(client.getState().lastError).toBe("Rejected: Stale decision nonce");
    client.stop();
  });

  it("routes the HTTP snapshot through the same validated reducer and closes its socket on stop", async () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: true, json: async () => snapshot }),
    );
    const client = new GameSessionClient({
      gameId: "game_12345",
      viewer: { role: "spectator" },
    });

    client.start();
    await vi.waitFor(() => expect(client.getState().gameVersion).toBe(4));
    client.stop();

    expect(client.getState().snapshot?.type).toBe("initial_snapshot");
    expect(client.getState().lastError).toBeNull();
    expect(FakeWebSocket.latest?.readyState).toBe(3);
  });

  it("subscribes as the authenticated player, pings every ten seconds, and resumes after disconnect", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          ...snapshot,
          viewer: { role: "player", seat: "player_a" },
        }),
      }),
    );
    const client = new GameSessionClient({
      gameId: "game_12345",
      viewer: { role: "player", seat: "player_a", playerSession: "private" },
    });
    client.start();
    const socket = FakeWebSocket.latest!;
    socket.readyState = FakeWebSocket.OPEN;
    socket.onopen?.();
    expect(JSON.parse(socket.sent[0])).toMatchObject({
      type: "subscribe",
      protocol_version: 4,
      player_session: "private",
    });
    vi.advanceTimersByTime(10_000);
    expect(JSON.parse(socket.sent[1])).toMatchObject({
      type: "ping",
      sequence: 1,
    });
    socket.onclose?.();
    vi.advanceTimersByTime(2_000);
    expect(FakeWebSocket.latest).not.toBe(socket);
    client.stop();
    vi.useRealTimers();
  });

  it("does not render a snapshot authenticated for a different player", async () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          ...snapshot,
          viewer: { role: "player", seat: "player_b" },
        }),
      }),
    );
    const client = new GameSessionClient({
      gameId: "game_12345",
      viewer: { role: "player", seat: "player_a", playerSession: "private" },
    });
    client.start();
    await vi.waitFor(() =>
      expect(client.getState().lastError).toMatch(/viewer identity/),
    );
    expect(client.getState().snapshot).toBeNull();
    client.stop();
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
