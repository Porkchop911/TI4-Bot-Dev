import { describe, expect, it } from "vitest";
import {
  contrastRatio,
  playerDisplay,
  seatInk,
  seatStyle,
  SEAT_COLORS,
  SEAT_INK_LIGHT,
  SEAT_SYMBOLS,
} from "./playerDisplay.ts";
import type { LobbyDto } from "../protocol/types.ts";

const lobby: LobbyDto = {
  game_id: "g",
  phase: "lobby",
  lobby_version: 1,
  host_player_id: "player_a",
  slots: [
    {
      slot_id: "slot_2",
      position: 1,
      occupant: "player_b",
      nickname: "Alex",
      connected: false,
      can_take_over: false,
    },
    {
      slot_id: "slot_1",
      position: 2,
      occupant: "player_a",
      nickname: "Alex",
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "slot_3",
      position: 3,
      occupant: null,
      nickname: null,
      connected: false,
      can_take_over: false,
    },
  ],
};

describe("physical seat identity", () => {
  it("pins the TI4-traditional palette in contrast-optimised seat order", () => {
    // blue, green, white, red, pink, orange, yellow, purple: blue/green first, red and orange apart.
    expect(SEAT_COLORS).toEqual([
      "#3B82F6",
      "#4ADE80",
      "#F1F5F9",
      "#EF4444",
      "#F472B6",
      "#F97316",
      "#F5D90A",
      "#A855F7",
    ]);
    expect(new Set(SEAT_COLORS).size).toBe(8);
    // Small games never seat the two reds (red = seat 4, orange = seat 6) together.
    expect(SEAT_COLORS.indexOf("#F97316")).toBeGreaterThanOrEqual(5);
    expect(new Set(SEAT_SYMBOLS).size).toBe(8);
    for (let position = 1; position <= 8; position++)
      expect(seatStyle(position)).toEqual({
        color: SEAT_COLORS[position - 1],
        symbol: SEAT_SYMBOLS[position - 1],
      });
  });

  it("uses current physical position, disambiguates duplicate names, and never exposes unknown IDs", () => {
    expect(playerDisplay(lobby, ["player_b", "player_a"], "player_a")).toMatchObject({
      label: "Alex (▲ Position 2)",
      position: 2,
      color: SEAT_COLORS[1],
    });
    const moved = {
      ...lobby,
      slots: lobby.slots.map((slot, index) => ({ ...slot, position: 2 - index || 3 })),
    };
    expect(playerDisplay(moved, [], "player_a").color).toBe(SEAT_COLORS[0]);
    expect(playerDisplay(lobby, [], "missing").label).toBe("Unknown participant");
    expect(playerDisplay(lobby, [], "player_b").label).toBe("Alex (● Position 1)");
    expect(playerDisplay(lobby, [], null).label).toBe("Unknown participant");
  });

  it("uses the frozen game order for missing position only and the refreshed nickname after takeover", () => {
    const renamed = {
      ...lobby,
      phase: "running" as const,
      slots: lobby.slots.map((slot) =>
        slot.occupant === "player_a" ? { ...slot, nickname: "Robin" } : slot,
      ),
    };
    expect(playerDisplay(renamed, ["player_b", "player_a"], "player_a").label).toBe("Robin");
    expect(playerDisplay(null, ["player_b", "player_a"], "player_a").label).toBe(
      "Participant at position 2",
    );
  });

  it("keeps every seat colour visible on the board and its symbol legible", () => {
    for (const color of SEAT_COLORS) {
      expect(contrastRatio(color, "#0f172a")).toBeGreaterThanOrEqual(4.5);
      // Documented rule: ink is whichever of dark/white reads better; it must clear 4.5:1.
      expect(contrastRatio(color, seatInk(color))).toBeGreaterThanOrEqual(4.5);
    }
    // No seat needs white ink any more (the old black seat 8 is gone); white and yellow need dark.
    expect(SEAT_COLORS.map(seatInk)).not.toContain(SEAT_INK_LIGHT);
    expect(seatInk("#000000")).toBe(SEAT_INK_LIGHT);
    expect(seatInk("#F1F5F9")).toBe("#0b1220");
  });
});
