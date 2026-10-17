import { describe, expect, it } from "vitest";
import type { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import {
  canConfirmReady,
  describeReadyPlanets,
  readyPlan,
  toggleReadyPlanet,
  validReadySelection,
} from "./readyPlanets.ts";

const ready = (...ids: string[]): PendingChoiceDto => ({
  actor: "b",
  nonce: "n1",
  prompt: "ready which planet",
  options: ids.map((id): ChoiceOption => ({ id, kind: "ready", label: `ready ${id}`, payload: {} })),
  context: { subtype: "ready_planet" } as PendingChoiceDto["context"],
});

describe("describeReadyPlanets", () => {
  it("lists every exhausted planet and owes two when three are offered", () => {
    const view = describeReadyPlanets(ready("jord", "lodor", "quann"))!;
    expect(view.planets.map((planet) => planet.id)).toEqual(["jord", "lodor", "quann"]);
    expect(view.required).toBe(2);
  });

  it("owes only what is exhausted when fewer than two are", () => {
    expect(describeReadyPlanets(ready("jord"))!.required).toBe(1);
    expect(describeReadyPlanets(ready("jord", "lodor"))!.required).toBe(2);
  });

  it("is null for any other decision", () => {
    expect(describeReadyPlanets(null)).toBeNull();
    expect(describeReadyPlanets({ ...ready("jord"), context: { subtype: "mine_planet" } as PendingChoiceDto["context"] })).toBeNull();
    const mixed = ready("jord");
    mixed.options.push({ id: "decline", kind: "decline", label: "decline", payload: {} });
    expect(describeReadyPlanets(mixed)).toBeNull();
  });
});

describe("selection state machine", () => {
  const view = describeReadyPlanets(ready("jord", "lodor", "quann"))!;

  it("toggles a planet in and out", () => {
    let picked: string[] = [];
    picked = toggleReadyPlanet(view, picked, "jord");
    expect(picked).toEqual(["jord"]);
    picked = toggleReadyPlanet(view, picked, "jord");
    expect(picked).toEqual([]);
  });

  it("swaps out the oldest pick when a third planet is tapped", () => {
    let picked = toggleReadyPlanet(view, [], "jord");
    picked = toggleReadyPlanet(view, picked, "lodor");
    picked = toggleReadyPlanet(view, picked, "quann");
    expect(picked).toEqual(["lodor", "quann"]);
  });

  it("ignores a planet the question does not offer", () => {
    expect(toggleReadyPlanet(view, ["jord"], "xxcha")).toEqual(["jord"]);
  });

  it("confirms only a complete selection and builds one step per planet in pick order", () => {
    expect(canConfirmReady(view, [])).toBe(false);
    expect(canConfirmReady(view, ["jord"])).toBe(false);
    expect(readyPlan(view, ["jord"])).toBeNull();
    expect(canConfirmReady(view, ["quann", "jord"])).toBe(true);
    expect(readyPlan(view, ["quann", "jord"])).toEqual({
      kind: "ready_planets",
      steps: [
        { kind: "ready_planet", planet: "quann" },
        { kind: "ready_planet", planet: "jord" },
      ],
    });
  });

  it("drops stale, repeated and surplus picks", () => {
    expect(validReadySelection(view, ["xxcha", "jord", "jord", "lodor", "quann"])).toEqual(["jord", "lodor"]);
  });

  it("a lone exhausted planet is a one-step plan", () => {
    const lone = describeReadyPlanets(ready("jord"))!;
    expect(readyPlan(lone, ["jord"])).toEqual({
      kind: "ready_planets",
      steps: [{ kind: "ready_planet", planet: "jord" }],
    });
  });
});
