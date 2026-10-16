import { describe, expect, it } from "vitest";
import { hydrateTech } from "./technologyData.ts";
import { matchesTechFilter, techColor, NO_TECH_FILTER } from "./techFilter.ts";

describe("technology filter", () => {
  it("names the colour of standard, unit upgrade and faction technologies", () => {
    expect(techColor(hydrateTech("amd"))).toBe("PROPULSION");
    expect(techColor(hydrateTech("nm"))).toBe("BIOTIC");
    expect(techColor(hydrateTech("cr2"))).toBe("UNIT_UPGRADE");
    // A faction technology reads its colour from its own card.
    expect(techColor(hydrateTech("so"))).toBe("CYBERNETIC");
  });

  it("combines the colour with availability", () => {
    const nm = hydrateTech("nm");
    expect(matchesTechFilter(nm, NO_TECH_FILTER, false)).toBe(true);
    expect(matchesTechFilter(nm, { color: "BIOTIC", availableOnly: false }, false)).toBe(true);
    expect(matchesTechFilter(nm, { color: "WARFARE", availableOnly: false }, true)).toBe(false);
    expect(matchesTechFilter(nm, { color: "BIOTIC", availableOnly: true }, false)).toBe(false);
    expect(matchesTechFilter(nm, { color: null, availableOnly: true }, true)).toBe(true);
  });
});
