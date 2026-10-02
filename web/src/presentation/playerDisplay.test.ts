import { describe, expect, it } from "vitest";
import { playerDisplay, seatStyle, SEAT_COLORS, SEAT_SYMBOLS } from "./playerDisplay.ts";
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
      ready: false,
      connected: false,
      can_take_over: false,
    },
    {
      slot_id: "slot_1",
      position: 2,
      occupant: "player_a",
      nickname: "Alex",
      ready: true,
      connected: true,
      can_take_over: false,
    },
    {
      slot_id: "slot_3",
      position: 3,
      occupant: null,
      nickname: null,
      ready: false,
      connected: false,
      can_take_over: false,
    },
  ],
};

describe("physical seat identity", () => {
  it("pins eight distinct colors and symbols by position, including black", () => {
    expect(SEAT_COLORS).toEqual([
      "#E69F00",
      "#56B4E9",
      "#009E73",
      "#F0E442",
      "#0072B2",
      "#D55E00",
      "#CC79A7",
      "#000000",
    ]);
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
});
