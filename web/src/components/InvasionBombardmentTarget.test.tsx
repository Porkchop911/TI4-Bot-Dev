import { describe, expect, it } from "vitest";
import { getInvasionEffectInfo } from "./InvasionOverlay.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

describe("bombardment target decision", () => {
  const choice = {
    nonce: "n-bomb",
    actor: "seat_1",
    prompt: "whose units on cealdri take the bombardment's next hits (2 hits)",
    context: { subtype: "bombardment_target" },
    options: [
      { id: "seat_2", label: "Bo's units", kind: "bombardment_target", payload: { planet: "cealdri" } },
    ],
  } as PendingChoiceDto;

  it("says it is a bombardment target pick and that only ground forces are hit", () => {
    const info = getInvasionEffectInfo(choice, "cealdri");
    expect(info.categoryLabel).toMatch(/Bombardment/);
    expect(info.description).toMatch(/only be assigned to ground forces/);
    expect(info.title).toBe(choice.prompt);
  });
});
