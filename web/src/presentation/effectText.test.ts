import { describe, expect, it } from "vitest";
import { describeActiveEffect, describePreview, type EffectLabels } from "./effectText.ts";
import type { ActiveEffectDto, EffectPreviewDto } from "../protocol/types.ts";

const labels: EffectLabels = {
  seat: (id) => ({ p1: "You", p2: "Bob", p3: "Cleo" })[id] ?? id,
  viewerSeat: "p1",
};

const preview = (extra: Partial<EffectPreviewDto>): EffectPreviewDto => ({
  option: "x",
  card: "sabo1",
  effect: "cancel_card",
  ...extra,
});

describe("describePreview", () => {
  it("says which card Sabotage cancels and whose it is", () => {
    expect(describePreview(preview({ seats: ["p2"], refs: ["fs1"] }), labels)).toBe(
      "Cancels Bob's Flank Speed. The card stays spent and its effect does not happen.",
    );
  });

  it("names the ship Direct Hit destroys, or says it is immune", () => {
    const hit = preview({
      effect: "destroy_unit",
      card: "dh1",
      seats: ["p2"],
      system: "27",
      units: [{ owner: "p2", unit_type: "dreadnought", count: 1 }],
    });
    expect(describePreview(hit, labels)).toBe(
      "Destroys Bob's dreadnought in System 27, the ship that just sustained damage.",
    );
    expect(describePreview({ ...hit, fizzles: "immune" }, labels)).toContain("cannot be destroyed by Direct Hit");
  });

  it("states how many of the incoming hits Shields Holding cancels", () => {
    const shields = preview({ effect: "cancel_hits", amount: 2, of: 5, system: "27" });
    expect(describePreview(shields, labels)).toBe(
      "Cancels 2 of the 5 hits about to be assigned in System 27, leaving 3.",
    );
    expect(describePreview({ ...shields, of: 1 }, labels)).toContain("so none are assigned");
    expect(describePreview({ ...shields, of: undefined }, labels)).toBe("Cancels up to 2 hits.");
  });

  it("counts what Deadly Plot exhausts and names the agenda", () => {
    expect(
      describePreview(
        preview({ effect: "discard_agenda", card: "deadly_plot", amount: 4, refs: ["minister_of_peace"] }),
        labels,
      ),
    ).toBe("Discards Minister of Peace with no effect and no replacement, then exhausts 4 ready planets of yours.");
  });

  it("says when playing it would change nothing", () => {
    expect(
      describePreview(preview({ effect: "repair_units", card: "emergency", fizzles: "nothing_there", system: "27" }), labels),
    ).toContain("would change nothing");
    expect(describePreview(preview({ effect: "replenish_commodities", amount: 0 }), labels)).toContain(
      "already full",
    );
  });

  it("ignores a kind it does not know", () => {
    expect(describePreview(preview({ effect: "from_the_future" }), labels)).toBeNull();
  });
});

describe("describeActiveEffect", () => {
  const effect = (extra: Partial<ActiveEffectDto>): ActiveEffectDto => ({
    seat: "p2",
    effect: "combat_roll_bonus",
    until: "combat_round",
    card: "mb1",
    ...extra,
  });

  it("gives the card, a short line and the expiry", () => {
    expect(describeActiveEffect(effect({ amount: 1 }), labels)).toEqual({
      title: "Morale Boost",
      summary: "+1 to all combat rolls",
      expiry: "until the end of this combat round",
    });
  });

  it("names the pool of hit cancellations without a single card", () => {
    const text = describeActiveEffect(
      effect({ effect: "cancel_hits", card: undefined, amount: 1, until: "combat_round" }),
      labels,
    );
    expect(text?.title).toBe("Shields Holding / Maneuvering Jets");
    expect(text?.summary).toBe("Cancels the next 1 hit");
  });

  it("names who holds a duress mark", () => {
    const text = describeActiveEffect(
      effect({ effect: "duress", card: "extremeduress", other: "p3", until: "next_action" }),
      labels,
    );
    expect(text?.summary).toContain("Marked by Cleo");
    expect(text?.expiry).toBe("until their next action");
  });

  it("skips a kind it does not know", () => {
    expect(describeActiveEffect(effect({ effect: "from_the_future" }), labels)).toBeNull();
  });
});
