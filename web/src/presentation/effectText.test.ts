import { describe, expect, it } from "vitest";
import { describeActiveEffect, describePreview, type EffectLabels } from "./effectText.ts";
import type { ActiveEffectDto, EffectPreviewDto } from "../protocol/types.ts";

// The engine words each effect (`reaction_preview.rs`, `view/effects.rs`, with their tests); this
// page only says the subjects that a phrase names.
const labels: EffectLabels = {
  seat: (id) => ({ p1: "You", p2: "Bob", p3: "Cleo" })[id] ?? id,
  viewerSeat: "p1",
};

const preview = (phrase: EffectPreviewDto["phrase"]): EffectPreviewDto => ({
  option: "x",
  card: "dh1",
  effect: "destroy_unit",
  phrase,
});

describe("describePreview", () => {
  it("says the seats, the units, the system and the cards of the engine's sentence", () => {
    const hit = preview({
      text: "Destroys {0} {1} in {2}, the ship that just sustained damage.",
      refs: {
        0: { kind: "seat_possessive", player: "p2" },
        1: { kind: "unit", unit_type: "dreadnought" },
        2: { kind: "system", system: "27" },
      },
    });
    expect(describePreview(hit, labels)).toBe("Destroys Bob's dreadnought in System 27, the ship that just sustained damage.");
    expect(describePreview(hit, { ...labels, system: (id) => `System ${id} (Lodor)` })).toContain("System 27 (Lodor)");
  });

  it("says you and your for the viewer", () => {
    const text = describePreview(
      preview({
        text: "{0}: {1} {2}, {3} and {4}.",
        refs: {
          0: { kind: "seat", player: "p1" },
          1: { kind: "seat_possessive", player: "p1" },
          2: { kind: "units", unit_type: "cruiser", count: 2 },
          3: { kind: "units", unit_type: "dreadnought", count: 1 },
          4: { kind: "action_card", card: "fs1" },
        },
      }),
      labels,
    );
    expect(text).toBe("You: your 2 cruisers, 1 dreadnought and Flank Speed.");
  });

  it("shows nothing for a preview without words", () => {
    expect(describePreview(preview({ text: "", refs: {} }), labels)).toBeNull();
  });
});

describe("describeActiveEffect", () => {
  const effect = (extra: Partial<ActiveEffectDto>): ActiveEffectDto => ({
    seat: "p2",
    effect: "duress",
    until: "next_action",
    card: "extremeduress",
    title: { text: "{0}", refs: { 0: { kind: "action_card", card: "mb1" } } },
    summary: { text: "Marked by {0}: a non-strategic action costs cards", refs: { 0: { kind: "seat", player: "p3" } } },
    expiry: "until their next action",
    ...extra,
  });

  it("gives the card, the line with its names and the expiry", () => {
    expect(describeActiveEffect(effect({}), labels)).toEqual({
      title: "Morale Boost",
      summary: "Marked by Cleo: a non-strategic action costs cards",
      expiry: "until their next action",
    });
  });

  it("skips an effect without words", () => {
    expect(describeActiveEffect(effect({ summary: { text: "", refs: {} } }), labels)).toBeNull();
  });
});
