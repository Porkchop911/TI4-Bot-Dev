import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import type { BoardView } from "../protocol/types.ts";
import { PlanetValue, PlanetValuePair } from "../components/PlanetValueIcons.tsx";
import {
  PlanetBoostContext,
  planetBoostMap,
  planetInfluenceMap,
  planetResourceMap,
} from "./PlanetResourcesContext.tsx";
import { deriveSelectedSystemDetails } from "./boardPresentation.ts";
import { getPlanetDetails } from "./planetSelection.ts";
import { applyBoosts } from "@ti4/shared/planets/values.ts";
import { boostLines, boostSummary } from "./planetBoostText.ts";

const MINING = { source: "Mining World", resources: 2, influence: 0 };
const DYSON = { source: "Dyson Sphere", resources: 2, influence: 1 };

function board(boosts: { source: string; resources: number; influence: number }[]): BoardView {
  return {
    map_tiles: [
      {
        system_id: "27",
        label: "Lodor",
        q: 0,
        r: 0,
        planets: [{ id: "lodor", label: "Lodor", resources: 3, influence: 1 }],
      },
    ],
    systems: {
      "27": {
        system_id: "27",
        command_tokens: [],
        units: [],
        planets: {
          lodor: { planet_id: "lodor", controlled_by: "a", exhausted: false, attachments: ["miningworld"], value_boosts: boosts },
        },
      },
    },
  } as unknown as BoardView;
}

describe("planet value boosts", () => {
  it("adds every boost to the printed values and lists them for the tooltip", () => {
    expect(applyBoosts(3, 1, [MINING, DYSON])).toEqual({ resources: 7, influence: 2 });
    expect(boostLines([MINING, DYSON], "resources")).toEqual([
      "Mining World: +2 resources",
      "Dyson Sphere: +2 resources",
    ]);
    expect(boostLines([MINING, DYSON], "influence")).toEqual(["Dyson Sphere: +1 influence"]);
    expect(boostSummary([MINING, DYSON])).toBe("Mining World +2R, Dyson Sphere +2R +1I");
  });

  it("makes the value maps and planet details effective, with the boosts kept", () => {
    const b = board([MINING]);
    expect(planetResourceMap(b).get("lodor")).toBe(5);
    expect(planetInfluenceMap(b).get("lodor")).toBe(1);
    expect(planetBoostMap(b).get("lodor")).toEqual([MINING]);
    const details = getPlanetDetails("lodor", b);
    expect(details.resources).toBe(5);
    expect(details.influence).toBe(1);
    expect(details.boosts).toEqual([MINING]);
    const planet = deriveSelectedSystemDetails("27", b, [], [], null)?.planets[0];
    expect([planet?.resources, planet?.influence, planet?.boosts]).toEqual([5, 1, [MINING]]);
  });

  it("leaves an unboosted planet as printed", () => {
    const b = board([]);
    expect(planetResourceMap(b).get("lodor")).toBe(3);
    expect(planetBoostMap(b).size).toBe(0);
  });
});

describe("boosted PlanetValue", () => {
  it("marks the number, shows +N and names the source in the label", () => {
    render(<PlanetValue kind="resources" value={5} boosts={[MINING]} testId="v" />);
    const el = screen.getByTestId("v");
    expect(el.getAttribute("data-boosted")).toBe("true");
    expect(el.className).toContain("planet-value--boosted");
    expect(el.textContent).toBe("5+2");
    expect(el.getAttribute("aria-label")).toBe("5 resources (3 printed; Mining World: +2 resources)");
    expect(el.getAttribute("title")).toBe(el.getAttribute("aria-label"));
  });

  it("only marks the kind the boost applies to", () => {
    render(<PlanetValuePair resources={5} influence={1} boosts={[MINING]} />);
    const [res, inf] = screen.getAllByRole("img");
    expect(res.getAttribute("data-boosted")).toBe("true");
    expect(inf.getAttribute("data-boosted")).toBeNull();
    expect(inf.textContent).toBe("1");
  });

  it("looks the boosts up by planet id from the context", () => {
    render(
      <PlanetBoostContext.Provider value={planetBoostMap(board([DYSON]))}>
        <PlanetValuePair resources={5} influence={2} planetId="lodor" />
        <PlanetValuePair resources={5} influence={2} planetId="elsewhere" />
      </PlanetBoostContext.Provider>,
    );
    const marks = screen.getAllByRole("img").map((el) => el.getAttribute("data-boosted"));
    expect(marks).toEqual(["true", "true", null, null]);
  });

  it("is unchanged without boosts", () => {
    render(<PlanetValue kind="influence" value={2} testId="v" />);
    expect(screen.getByTestId("v").getAttribute("aria-label")).toBe("2 influence");
    expect(screen.getByTestId("v").getAttribute("data-boosted")).toBeNull();
  });
});
