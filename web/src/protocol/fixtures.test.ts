import { describe, it, expect } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";
import { InitialSnapshotMsg, ActionRejectedMsg, GameOverMsg, PROTOCOL_VERSION } from "./types.ts";
import { decodeServerMessage, isStaleServerMessage } from "./decode.ts";

const FIXTURES_DIR = path.resolve(__dirname, "../../../crates/ti4-server/fixtures");

function loadFixture<T>(filename: string): T {
  const filePath = path.join(FIXTURES_DIR, filename);
  const raw = fs.readFileSync(filePath, "utf-8");
  return JSON.parse(raw) as T;
}

describe("Golden Fixtures Conformance", () => {
  it("parses actor_snapshot.json and preserves private cards and pending choice", () => {
    const data = loadFixture<InitialSnapshotMsg>("actor_snapshot.json");
    expect(data.protocol_version).toBe(PROTOCOL_VERSION);
    expect(data.game_id).toBe("game_12345");
    expect(data.viewer).toEqual({ role: "player", seat: "seat_a" });
    expect(data.pending_choice).not.toBeNull();
    expect(data.pending_choice?.choice.player).toBe("seat_a");
    expect(data.pending_choice?.choice.options.length).toBeGreaterThan(0);

    const actor = data.view.players.find((p) => p.id === "seat_a");
    expect(actor).toBeDefined();
    expect(actor?.held_action_cards?.length).toBeGreaterThan(0);
    expect(actor?.held_secret_objectives?.length).toBeGreaterThan(0);
  });

  it("parses opponent_snapshot.json and confirms redaction of actor private cards", () => {
    const data = loadFixture<InitialSnapshotMsg>("opponent_snapshot.json");
    expect(data.protocol_version).toBe(PROTOCOL_VERSION);
    expect(data.viewer).toEqual({ role: "player", seat: "seat_b" });
    // Opponent cannot see seat_a's choice
    expect(data.pending_choice).toBeUndefined();

    const actor = data.view.players.find((p) => p.id === "seat_a");
    expect(actor).toBeDefined();
    expect(actor?.held_action_cards).toBeUndefined();
    expect(actor?.held_secret_objectives).toBeUndefined();
  });

  it("parses spectator_snapshot.json and verifies spectator role and full redaction", () => {
    const data = loadFixture<InitialSnapshotMsg>("spectator_snapshot.json");
    expect(data.protocol_version).toBe(PROTOCOL_VERSION);
    expect(data.viewer).toEqual({ role: "spectator" });
    expect(data.pending_choice).toBeUndefined();

    for (const player of data.view.players) {
      expect(player.held_action_cards).toBeUndefined();
      expect(player.held_secret_objectives).toBeUndefined();
    }
  });

  it("parses stale_submission_rejected.json with structured rejection reason", () => {
    const data = loadFixture<ActionRejectedMsg>("stale_submission_rejected.json");
    expect(data.protocol_version).toBe(PROTOCOL_VERSION);
    expect(data.game_id).toBe("game_12345");
    expect(data.game_version).toBe(42);
    expect(data.reason).toEqual({
      reason: "stale_version",
      expected: 40,
      current: 42,
    });
  });

  it("parses terminal_game_over.json with winner and final scores", () => {
    const data = loadFixture<GameOverMsg>("terminal_game_over.json");
    expect(data.protocol_version).toBe(PROTOCOL_VERSION);
    expect(data.game_id).toBe("game_12345");
    expect(data.winner).toBe("seat_a");
    expect(data.final_scores["seat_a"]).toBe(10);
    expect(data.final_scores["seat_b"]).toBe(8);
  });

  it("rejects malformed, wrong-version, and wrong-game messages at ingress", () => {
    const snapshot = loadFixture<Record<string, unknown>>("actor_snapshot.json");
    expect(() => decodeServerMessage({ type: "initial_snapshot" }, "game_12345")).toThrow(
      /invalid server message/i,
    );
    expect(decodeServerMessage(snapshot, "game_12345").type).toBe("initial_snapshot");
    expect(() => decodeServerMessage({ ...snapshot, protocol_version: 2 }, "game_12345")).toThrow(
      /unsupported protocol version/i,
    );
    expect(() => decodeServerMessage({ ...snapshot, protocol_version: 99 }, "game_12345")).toThrow(
      /unsupported protocol version/i,
    );
    expect(() =>
      decodeServerMessage(
        { ...snapshot, protocol_version: PROTOCOL_VERSION, game_id: "other_game" },
        "game_12345",
      ),
    ).toThrow(/unexpected game id/i);
  });

  it("rejects state updates older than the current game version", () => {
    const update = decodeServerMessage(
      {
        ...loadFixture<Record<string, unknown>>("actor_snapshot.json"),
        type: "state_update",
        game_version: 41,
      },
      "game_12345",
    );
    expect(isStaleServerMessage(update, 42)).toBe(true);
    expect(isStaleServerMessage(update, 41)).toBe(false);
  });
});
