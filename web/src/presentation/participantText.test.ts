import { describe, expect, it } from "vitest";
import type { LobbyDto } from "../protocol/types.ts";
import { participantText } from "./participantText.ts";

const alice = `player_${"a".repeat(64)}`;
const missing = `player_${"c".repeat(64)}`;
const bob = `player_${"b".repeat(64)}`;
const roster: LobbyDto = {
  game_id: "game",
  phase: "running",
  lobby_version: 1,
  host_player_id: alice,
  slots: [
    {
      slot_id: "slot_1",
      position: 1,
      occupant: alice,
      nickname: "Alex",
      ready: true,
      connected: false,
      can_take_over: false,
    },
    {
      slot_id: "slot_2",
      position: 2,
      occupant: bob,
      nickname: "Alex",
      ready: true,
      connected: false,
      can_take_over: false,
    },
  ],
};

describe("participant prose boundary", () => {
  it("resolves standalone references in prompts, option labels and errors, including unknown IDs", () => {
    const text = `${alice}'s offer to ${bob} -- accept? Unknown ${missing}.`;
    expect(participantText(text, roster, [alice, bob])).toBe(
      `Alex (● Position 1)'s offer to Alex (▲ Position 2) -- accept? Unknown Unknown participant.`,
    );
  });

  it("does not replace substrings, composite content IDs or unrelated tokens", () => {
    const text = `pre${alice}post item:${alice} ${alice}-variant path/${alice} ${alice}|suffix player_not_a_generated_id`;
    expect(participantText(text, roster, [alice, bob])).toBe(text);
  });

  it("reads the latest roster at render time without changing the original text", () => {
    const original = `${alice} offers ${bob}`;
    const renamed = {
      ...roster,
      slots: roster.slots.map((slot) =>
        slot.occupant === bob ? { ...slot, nickname: "Bee" } : slot,
      ),
    };
    expect(participantText(original, renamed, [alice, bob])).toBe("Alex offers Bee");
    expect(original).toContain(bob);
  });
});
