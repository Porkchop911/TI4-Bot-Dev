import { describe, expect, it } from "vitest";
import { decodeSplicePreview } from "./splice.ts";

const counters = { dice_rolls: 0, dice_faces: 0, decks: {} };

const survives = {
  edit: { kind: "replace", cursor: 44, option_id: "fleet_tokens" },
  original_decisions: 400,
  later_decisions: 355,
  kept_later: 355,
  dropped_later: 0,
  survives_to_end: true,
  kept_reordered: [],
  first_conflict: null,
  alignment: [],
  rng: {
    status: "neutral",
    compared: 12,
    removed_consumed: null,
    first_divergence_cursor: null,
    first_divergence: null,
  },
};

const conflict = {
  cursor: 8,
  kind: "prompt",
  soft: false,
  seat: "p3",
  prompt: "movement",
  expected_chosen: "done_moving",
  expected_offered: ["done_moving"],
  found_seat: "p3",
  found_prompt: "activate a system",
  found_offered: ["36", "19"],
  added: [],
  removed: [],
  detail: "recorded movement, engine asked activate a system",
  asks_removed_decision: true,
};

describe("decodeSplicePreview", () => {
  it("decodes a report where everything survives", () => {
    const preview = decodeSplicePreview(survives);
    expect(preview.survives_to_end).toBe(true);
    expect(preview.first_conflict).toBeNull();
    expect(preview.rng.status).toBe("neutral");
  });

  it("decodes a conflict, alignment notes and rng counters", () => {
    const preview = decodeSplicePreview({
      ...survives,
      edit: { kind: "remove", cursor: 7 },
      survives_to_end: false,
      kept_later: 0,
      dropped_later: 392,
      first_conflict: conflict,
      alignment: [
        { change: "appeared", cursor: 97, seat: "p1", prompt: "pay 3 more resources", option_id: "exhaust|jord" },
      ],
      rng: {
        status: "not_neutral",
        compared: 1,
        removed_consumed: { ...counters, decks: { action_card: -2 } },
        first_divergence_cursor: 71,
        first_divergence: { ...counters, dice_faces: 3 },
      },
    });
    expect(preview.first_conflict?.kind).toBe("prompt");
    expect(preview.first_conflict?.asks_removed_decision).toBe(true);
    expect(preview.alignment[0].change).toBe("appeared");
    expect(preview.rng.removed_consumed?.decks.action_card).toBe(-2);
    expect(preview.rng.first_divergence?.dice_faces).toBe(3);
  });

  it("rejects malformed reports", () => {
    expect(() => decodeSplicePreview(null)).toThrow();
    expect(() => decodeSplicePreview({ ...survives, kept_later: -1 })).toThrow();
    expect(() => decodeSplicePreview({ ...survives, edit: { kind: "squash", cursor: 1 } })).toThrow();
    expect(() =>
      decodeSplicePreview({ ...survives, first_conflict: { ...conflict, kind: "mystery" } }),
    ).toThrow();
    expect(() =>
      decodeSplicePreview({ ...survives, rng: { ...survives.rng, status: "maybe" } }),
    ).toThrow();
  });
});
