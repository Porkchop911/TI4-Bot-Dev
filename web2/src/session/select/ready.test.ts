import { describe, expect, it } from "vitest";
import type { Choice, SessionUpdate } from "../wire";
import {
  chooseReadySystem,
  clearLastReady,
  clearReadyPart,
  selectReady,
  toggleReadyPlanet,
} from "./ready";

// The questions of Diplomacy as the engine asks them (the scenario "strategy_diplomacy").
const HINTS = {
  planets: [
    { id: "jord", name: "Jord", resources: 4, influence: 2 },
    { id: "arinam", name: "Arinam", resources: 1, influence: 2 },
    { id: "meer", name: "Meer", resources: 0, influence: 4 },
  ],
  spenders: [
    { card: "Leadership", spends: "influence", status: "played" },
    { card: "Technology", spends: "resources", status: "to_come" },
    { card: "Warfare", spends: "resources", status: "not_in_play" },
  ],
};
const system: Choice = {
  player: "a",
  prompt: "choose a system for Diplomacy",
  options: ["01", "29", "37"].map((id) => ({ id, kind: "system", label: id, payload: {} })),
  details: { ready_hints: HINTS, ready_count: 2 },
};
const window: Choice = {
  player: "b",
  prompt: "spend a strategy token to ready two planets",
  options: [
    { id: "no", kind: "strategy", label: "decline", payload: {} },
    { id: "yes", kind: "strategy", label: "ready", payload: {} },
  ],
  details: {
    kind: "strategy_secondary",
    ready_hints: HINTS,
    ready_count: 2,
    tokens_left: 2,
    costs_token: true,
  },
};
const ready: Choice = {
  player: "a",
  prompt: "ready which planet",
  options: ["jord", "meer"].map((id) => ({
    id,
    kind: "ready",
    label: `ready ${id}`,
    payload: { planet: id },
  })),
  details: { ready_hints: HINTS, ready_count: 1 },
};
const update = {
  viewer: { role: "player", seat: "a" },
  view: {
    players: [{ id: "a" }, { id: "b" }, { id: "c" }],
    board: {
      map_tiles: [
        { system_id: "01", planets: [{ id: "jord", label: "Jord" }] },
        {
          system_id: "37",
          planets: [
            { id: "arinam", label: "Arinam" },
            { id: "meer", label: "Meer" },
          ],
        },
      ],
    },
  },
} as unknown as SessionUpdate;

describe("selectReady", () => {
  it("is null for a question with no planets to ready", () => {
    expect(selectReady(update, { ...system, details: {} }, null)).toBeNull();
    expect(selectReady(update, { ...window, details: { kind: "strategy_secondary" } }, null)).toBe(
      null,
    );
  });

  it("chooses the system on the map first, then the planets, and answers all at once", () => {
    const empty = selectReady(update, system, null)!;
    expect(empty.task).toMatchObject({ target: "system", values: { "01": 0, "29": 0, "37": 0 } });
    expect(empty.problem).toBe("Choose a system on the map.");
    expect(empty.label).toBe("Resolve primary");
    expect(toggleReadyPlanet(system, null, "jord")).toBeNull();

    let draft = chooseReadySystem(system, null, "37")!;
    expect(chooseReadySystem(system, draft, "18")).toBeNull();
    draft = toggleReadyPlanet(system, draft, "jord")!;
    draft = toggleReadyPlanet(system, draft, "meer")!;
    // No more than the game readies.
    expect(toggleReadyPlanet(system, draft, "arinam")).toBeNull();
    const view = selectReady(update, system, draft)!;
    expect(view.task).toMatchObject({ target: "planet", chosen: { jord: true, meer: true } });
    expect(view.blocks[1]).toMatchObject({
      title: "Ready planets",
      aside: "2 / 2 · Choose on the map",
    });
    expect(view.blocks[2]).toMatchObject({
      kind: "note",
      text: "Leadership played (influence) · Technology to come (resources)",
    });
    expect(view.problem).toBeNull();
    expect(view.answers).toEqual(["37", "jord", "meer"]);

    // "Change" and Esc.
    expect(clearReadyPart(system, draft, "planet:jord")).toEqual({
      system: "37",
      planets: ["meer"],
    });
    expect(clearReadyPart(system, draft, "system")).toEqual({
      system: null,
      planets: ["jord", "meer"],
    });
    expect(clearLastReady(system, draft)).toEqual({ system: "37", planets: ["jord"] });
    expect(clearLastReady(system, { system: "37", planets: [] })).toEqual({
      system: null,
      planets: [],
    });
  });

  it("a secondary with nothing chosen is a pass, and says what it costs", () => {
    const pass = selectReady(update, window, null)!;
    expect(pass.label).toBe("Pass");
    expect(pass.answers).toEqual(["no"]);
    expect(pass.blocks[0]).toMatchObject({
      aside: "0 / 2 · Choose on the map · Strategy pool 2 → 1",
    });
    const one = toggleReadyPlanet(window, null, "arinam")!;
    expect(selectReady(update, window, one)!.problem).toBe("Choose 1 more planet on the map.");
    const two = toggleReadyPlanet(window, one, "jord")!;
    const view = selectReady(update, window, two)!;
    expect(view.label).toBe("Follow · ready 2 planets");
    expect(view.answers).toEqual(["yes", "arinam", "jord"]);
  });

  it("a lone question of a planet readies what it says, of the planets it offers", () => {
    const view = selectReady(update, ready, toggleReadyPlanet(ready, null, "meer"))!;
    expect(view.task).toMatchObject({ target: "planet", values: { jord: 4, meer: 0 } });
    expect(toggleReadyPlanet(ready, null, "arinam")).toBeNull();
    expect(view.answers).toEqual(["meer"]);
  });
});
