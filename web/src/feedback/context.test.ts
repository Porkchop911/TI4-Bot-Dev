import { describe, expect, it } from "vitest";
import { buildFeedbackContext, recentPublicEvents, type FeedbackEnvironment } from "./context.ts";
import type { GameEvent, PendingChoiceDto } from "../protocol/types.ts";
import type { SnapshotState } from "../protocol/client.ts";

const env: FeedbackEnvironment = {
  commit: "abc1234",
  serverProtocol: 3,
  clientProtocol: 3,
  viewport: { width: 390, height: 844, pixelRatio: 3 },
  userAgent: "UA/1.0",
  now: new Date("2026-10-12T10:00:00Z"),
  path: "/games/game_deadbeef",
};

const event = (id: string, visibility: GameEvent["visibility"], extra: Partial<GameEvent> = {}): GameEvent =>
  ({
    id,
    timestamp: "t",
    visibility,
    event: { kind: "decision_resolved" },
    ...extra,
  }) as GameEvent;

// A snapshot whose `state` (the engine state passed through) holds another seat's secrets.
const snapshot = {
  type: "initial_snapshot",
  protocol_version: 3,
  game_id: "game_deadbeef",
  game_version: 7,
  viewer: { role: "player", seat: "p1", playerSession: "SECRET-TOKEN" },
  view: {
    round: 2,
    phase: "action",
    speaker: "p1",
    active_player: "p2",
    players: [
      { id: "p1", faction: "arborec", secret_hand: ["MY-CARD"] },
      { id: "p2", faction: "letnev", secret_hand: ["THEIR-HIDDEN-CARD"] },
    ],
  },
  state: { players: { p2: { hand: ["THEIR-HIDDEN-CARD"], secret_objectives: ["OTHER-SECRET"] } } },
} as unknown as SnapshotState;

const pending = {
  prompt: "Choose a system to activate",
  actor: "p1",
  nonce: "NONCE-SECRET",
  options: [
    { id: "activate:1", label: "Activate system 1" },
    { id: "pass", label: "Pass" },
  ],
  context: { subtype: "tactical_activation" },
} as unknown as PendingChoiceDto;

describe("buildFeedbackContext", () => {
  it("collects build, seat, faction, phase, the open decision and public events", () => {
    const ctx = buildFeedbackContext(
      {
        gameId: "game_deadbeef",
        viewer: { role: "player", seat: "p1", playerSession: "SECRET-TOKEN" },
        snapshot,
        pendingChoice: pending,
        events: [event("e1", { visibility: "public" }, { round: 2, phase: "action", detail: "p2 passed" })],
      },
      env,
      true,
    );
    expect(ctx).toMatchObject({
      build_commit: "abc1234",
      server_protocol_version: 3,
      game_id: "game_deadbeef",
      viewer_seat: "p1",
      faction: "arborec",
      round: 2,
      phase: "action",
      pending_decision: {
        subtype: "tactical_activation",
        prompt: "Choose a system to activate",
        option_ids: ["activate:1", "pass"],
      },
      viewport: { width: 390, height: 844, pixelRatio: 3 },
      user_agent: "UA/1.0",
      timestamp: "2026-10-12T10:00:00.000Z",
      path: "/games/:id",
    });
  });

  it("never carries the session token, the nonce, raw state or anything about other seats' hidden cards", () => {
    const ctx = buildFeedbackContext(
      {
        gameId: "game_deadbeef",
        viewer: { role: "player", seat: "p1", playerSession: "SECRET-TOKEN" },
        snapshot,
        pendingChoice: pending,
        events: [
          event("e1", { visibility: "seat", seat: "p2" }, { detail: "p2 drew THEIR-HIDDEN-CARD", private_detail: "OTHER-SECRET" }),
          event("e2", { visibility: "referee" }, { detail: "referee only" }),
          event("e3", { visibility: "public" }, { detail: "ok", private_detail: "OTHER-SECRET" }),
        ],
      },
      env,
      true,
    );
    const text = JSON.stringify(ctx);
    for (const secret of ["SECRET-TOKEN", "NONCE-SECRET", "THEIR-HIDDEN-CARD", "OTHER-SECRET", "MY-CARD", "referee only"]) {
      expect(text).not.toContain(secret);
    }
    expect(ctx.recent_public_events).toEqual([
      expect.objectContaining({ id: "e3", detail: "ok" }),
    ]);
  });

  it("keeps only the last 20 public events", () => {
    const events = Array.from({ length: 35 }, (_, i) => event(`e${i}`, { visibility: "public" }));
    const recent = recentPublicEvents(events);
    expect(recent).toHaveLength(20);
    expect(recent[0].id).toBe("e15");
    expect(recent[19].id).toBe("e34");
  });

  it("sends no game info at all when unticked, only the build and the screen", () => {
    const ctx = buildFeedbackContext(
      { gameId: "game_deadbeef", viewer: { role: "spectator" }, snapshot, pendingChoice: pending },
      env,
      false,
    );
    expect(Object.keys(ctx).sort()).toEqual(
      ["build_commit", "client_protocol_version", "path", "server_protocol_version", "timestamp", "user_agent", "viewport"].sort(),
    );
  });

  it("a spectator gets no seat or faction", () => {
    const ctx = buildFeedbackContext({ gameId: "g", viewer: { role: "spectator" }, snapshot }, env, true);
    expect(ctx.viewer_role).toBe("spectator");
    expect(ctx).not.toHaveProperty("viewer_seat");
    expect(ctx).not.toHaveProperty("faction");
  });

  describe("sender names", () => {
    const lobby = {
      slots: [
        { slot_id: "s1", position: 1, occupant: "p1", nickname: "Alex", ready: true, connected: true, can_take_over: false },
        { slot_id: "s2", position: 2, occupant: "p2", nickname: "np", ready: true, connected: true, can_take_over: false },
        { slot_id: "s3", position: 3, occupant: "b1", nickname: "Bot 3", ready: true, connected: true, can_take_over: false, bot: "smart" },
      ],
    } as unknown as import("../protocol/types.ts").LobbyDto;
    const viewer = { role: "player", seat: "p1", playerSession: "SECRET-TOKEN" } as const;
    const snap = {
      ...(snapshot as object),
      view: { ...(snapshot as unknown as { view: object }).view, players: [
        { id: "p1", faction: "arborec" }, { id: "p2", faction: "letnev" }, { id: "b1", faction: "sol" },
      ] },
    } as unknown as SnapshotState;

    it("records the sender's nickname and every seat's nickname and faction", () => {
      const ctx = buildFeedbackContext({ gameId: "g", viewer, snapshot: snap, lobby }, env, true);
      expect(ctx.sender).toEqual({ nickname: "Alex", position: 1 });
      expect(ctx.players).toEqual([
        { seat: "p1", position: 1, nickname: "Alex", bot: null, faction: "arborec", is_sender: true },
        { seat: "p2", position: 2, nickname: "np", bot: null, faction: "letnev", is_sender: false },
        { seat: "b1", position: 3, nickname: "Bot 3", bot: "smart", faction: "sol", is_sender: false },
      ]);
      expect(JSON.stringify(ctx)).not.toContain("SECRET-TOKEN");
    });

    it("adds the remembered lobby nickname only when it differs", () => {
      expect(buildFeedbackContext({ gameId: "g", viewer, lobby }, { ...env, storedNickname: "Alex" }, true).sender).toEqual({
        nickname: "Alex",
        position: 1,
      });
      expect(buildFeedbackContext({ gameId: "g", viewer, lobby }, { ...env, storedNickname: "Al" }, true).sender).toEqual({
        nickname: "Alex",
        position: 1,
        lobby_nickname: "Al",
      });
    });

    it("keeps the typed name for a sender without a nickname, even with game info unticked", () => {
      const ctx = buildFeedbackContext({ viewer: { role: "spectator" } }, { ...env, typedName: "  Sam " }, false);
      expect(ctx.sender).toEqual({ typed_name: "Sam" });
      expect(ctx).not.toHaveProperty("players");
    });

    it("leaves the other nicknames out when game info is unticked", () => {
      const ctx = buildFeedbackContext({ gameId: "g", viewer, snapshot: snap, lobby }, env, false);
      expect(ctx.sender).toEqual({ nickname: "Alex", position: 1 });
      expect(ctx).not.toHaveProperty("players");
    });
  });
});
