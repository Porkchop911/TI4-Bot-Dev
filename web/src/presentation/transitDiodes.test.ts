import { describe, expect, it } from "vitest";
import { galleryBoard } from "../dev/galleryBoard.ts";
import { transitChoice, transitOption } from "./transitDiodes.fixtures.ts";
import {
  buildTransitModel,
  describeTransitMove,
  findTransitForce,
  isTransitDiodesChoice,
  parseTransitOption,
} from "./transitDiodes.ts";

// Gallery board: the actor controls Jord + Exhausted World (#18), Wellon (#19), Tar'Mann (#22) and
// Sol's Jord (#1); infantry on Jord, Tar'Mann, Sol's Jord (2) and in #18's space area.
const destinations: [string, string][] = [
  ["18", "jord"],
  ["18", "exhausted"],
  ["19", "wellon"],
  ["22", "tarmann"],
  ["1", "sol_jord"],
];
const sources: [string, string, string][] = [
  ["1", "sol_jord", "infantry"],
  ["18", "space", "infantry"],
  ["18", "jord", "infantry"],
  ["22", "tarmann", "infantry"],
];
const all = sources.flatMap(([ss, source, unit]) =>
  destinations.filter(([ds, p]) => !(ds === ss && p === source)).map(([ds, p]) => transitOption(ss, source, unit, ds, p)),
);

describe("parseTransitOption", () => {
  it("reads the payload and falls back to the id parts", () => {
    expect(parseTransitOption(transitOption("18", "jord", "mech", "26", "lodor"))).toEqual({
      optionId: "transit|18|jord|mech|26|lodor",
      sourceSystem: "18",
      source: "jord",
      unit: "mech",
      destinationSystem: "26",
      planet: "lodor",
    });
    expect(parseTransitOption({ id: "transit|18|space|infantry|19|wellon", label: "", kind: "transit" })?.source).toBe("space");
    expect(parseTransitOption({ id: "decline", label: "", kind: "decline" })).toBeNull();
  });
});

describe("isTransitDiodesChoice", () => {
  it("recognises the redeploy question by subtype or by its option ids", () => {
    expect(isTransitDiodesChoice(transitChoice(all))).toBe(true);
    expect(isTransitDiodesChoice({ options: all, context: undefined })).toBe(true);
    expect(isTransitDiodesChoice({ options: [{ id: "jord", label: "Jord", kind: "planet", payload: { planet: "jord" } }] })).toBe(false);
    expect(isTransitDiodesChoice(transitChoice([]))).toBe(false);
  });
});

describe("buildTransitModel", () => {
  const model = buildTransitModel(transitChoice(all), galleryBoard);

  it("groups sources by system tag, then places with their forces and counts", () => {
    expect(model.movesLeft).toBe(3);
    expect(model.moves.size).toBe(all.length);
    expect(model.sources.map((g) => g.tag)).toEqual(["#01 [Sol Home]", "#18 [Mecatol Rex]", "#22 [Tar'Mann]"]);
    const mecatol = model.sources[1].items;
    expect(mecatol.map((s) => s.name)).toEqual(["Space area", "Jord"]);
    expect(mecatol[1]).toMatchObject({ resources: 2, influence: 2 });
    const sol = model.sources[0].items[0];
    expect(sol.forces[0]).toMatchObject({ unit: "infantry", count: 2, unitName: "Infantry" });
    expect(model.declineOption?.id).toBe("decline");
  });

  it("lists destinations per force, own system first, with what stands there", () => {
    const picked = findTransitForce(model, "22|tarmann|infantry")!;
    expect(picked.source.name).toBe("Tar'Mann");
    expect(picked.force.destinations.map((g) => g.tag)).toEqual(["#01 [Sol Home]", "#18 [Mecatol Rex]", "#19 [Wellon]"]);
    const jord = picked.force.destinations[1].items.find((d) => d.planetId === "jord")!;
    expect(jord).toMatchObject({ name: "Jord", resources: 2, influence: 2, here: "1 infantry", optionId: "transit|22|tarmann|infantry|18|jord" });
    expect(picked.force.destinations[2].items[0].here).toBeNull();
    // Jord's own infantry: Exhausted World (same system) comes first, Jord itself is not offered.
    const fromJord = findTransitForce(model, "18|jord|infantry")!;
    expect(fromJord.force.destinations[0].systemId).toBe("18");
    expect(fromJord.force.destinations[0].items.map((d) => d.planetId)).toEqual(["exhausted"]);
  });

  it("keeps every engine id reachable exactly once", () => {
    const reachable = model.sources
      .flatMap((g) => g.items)
      .flatMap((s) => s.forces)
      .flatMap((f) => f.destinations.flatMap((g) => g.items.map((d) => d.optionId)));
    expect([...reachable].sort()).toEqual(all.map((o) => o.id).sort());
  });
});

describe("describeTransitMove", () => {
  it("names unit, both planets and their systems", () => {
    const move = parseTransitOption(transitOption("18", "space", "infantry", "19", "wellon"))!;
    expect(describeTransitMove(move, galleryBoard)).toBe(
      "Move 1 Infantry: space area of #18 [Mecatol Rex] → Wellon (#19 [Wellon])",
    );
    const mech = parseTransitOption(transitOption("1", "sol_jord", "mech", "18", "jord"))!;
    expect(describeTransitMove(mech, galleryBoard)).toBe("Move 1 Mech: Jord (#01 [Sol Home]) → Jord (#18 [Mecatol Rex])");
  });
});
