import { describe, expect, it } from "vitest";
import {
  deriveBarrageWindow,
  deriveHitKind,
  hitDetailOf,
  illegalTargetReason,
  isBarrageWindow,
} from "./hitKind.ts";

describe("deriveHitKind", () => {
  it("Assault Cannon is a destroy that only non-fighter ships can take", () => {
    const model = deriveHitKind(
      {
        subtype: "assault_cannon_destroy",
        hit: { cause: "assault_cannon", destroy: true, restriction: "non_fighter" },
      },
      { producer: "Ann" },
    )!;
    expect(model.source).toBe("Assault Cannon");
    expect(model.tone).toBe("destroy");
    expect(model.headline).toBe("Assault Cannon: Ann destroys 1 of your non-fighter ships");
    expect(model.chips.map((c) => c.text)).toEqual([
      "Destroy, not a hit",
      "Non-fighter ships only",
      "Cannot be sustained",
    ]);
    expect(illegalTargetReason(model, "fighter")).toMatch(/cannot be assigned/i);
    expect(illegalTargetReason(model, "fighter_ii")).toMatch(/cannot be assigned/i);
    expect(illegalTargetReason(model, "carrier")).toBeNull();
  });

  it("Courageous to the End destroys any ship and cannot be sustained", () => {
    const model = deriveHitKind(
      { subtype: "courageous_to_the_end_assign_casualty" },
      { producer: "Bo" },
    )!;
    expect(model.headline).toContain("Courageous to the End");
    expect(model.tone).toBe("destroy");
    expect(model.restriction).toBe("any");
    expect(illegalTargetReason(model, "fighter")).toBeNull();
  });

  it("a Waylay-widened barrage hit is a normal hit on any ship, counted", () => {
    const model = deriveHitKind(
      {
        subtype: "assign_casualty",
        hit: { cause: "anti_fighter_barrage", destroy: false, restriction: "any" },
      },
      { producer: "Cy", hits: 2 },
    )!;
    expect(model.headline).toBe("Anti-Fighter Barrage: 2 hits from Cy to assign");
    expect(model.tone).toBe("hit");
    expect(model.chips.map((c) => c.text)).toContain("Any ship (Waylay)");
  });

  it("Space Cannon hits bound to non-fighters (Graviton) say so", () => {
    const model = deriveHitKind(
      {
        subtype: "assign_casualty",
        hit: { cause: "space_cannon", destroy: false, restriction: "non_fighter" },
      },
      { hits: 1 },
    )!;
    expect(model.headline).toBe("Space Cannon: 1 hit from your opponent to assign");
    expect(illegalTargetReason(model, "fighter")).not.toBeNull();
  });

  it("says nothing about an ordinary decision with no hit detail", () => {
    expect(deriveHitKind({ subtype: "assign_casualty" }, {})).toBeNull();
    expect(hitDetailOf(null)).toBeNull();
  });
});

describe("deriveBarrageWindow", () => {
  it("recognises the window by trigger or subtype", () => {
    expect(
      isBarrageWindow({
        subtype: "x",
        trigger: { kind: "anti_fighter_barrage", event_type: "", event_id: 1, relation: "when" },
      }),
    ).toBe(true);
    expect(isBarrageWindow({ subtype: "reaction_when_ANTI_FIGHTER_BARRAGE_STARTED" })).toBe(true);
    expect(isBarrageWindow({ subtype: "reaction_after_SPACE_COMBAT_WON" })).toBe(false);
  });

  it("names the roller and says the hits are automatic fighter kills", () => {
    const model = deriveBarrageWindow(
      { subtype: "reaction_when_ANTI_FIGHTER_BARRAGE_STARTED" },
      "Ann",
    )!;
    expect(model.headline).toBe("Anti-Fighter Barrage: before Ann rolls");
    expect(model.lines.join(" ")).toMatch(/destroys one of the opposing fighters/);
    expect(model.lines.join(" ")).toMatch(/Waylay/);
  });
});
