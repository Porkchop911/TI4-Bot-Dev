import { describe, it, expect } from "vitest";
import { computePlayerStats, isShip } from "./playerStats.ts";
import { BoardView, PlayerView } from "../protocol/types.ts";

describe("playerStats", () => {
  const basePlayer: PlayerView = {
    id: "p1",
    faction: "Federation of Sol",
    victory_points: 0,
    trade_goods: 0,
    commodities: 0,
    tactic_tokens: 3,
    fleet_tokens: 3,
    strategic_tokens: 2,
    passed: false,
    strategy_cards: [],
    exhausted_strategy_cards: [],
    technologies: [],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 0,
    secret_objectives_count: 0,
    leaders: {},
  };

  it("handles null or empty board gracefully", () => {
    const stats = computePlayerStats(basePlayer, null);
    expect(stats).toEqual({
      remainingResources: 0,
      totalResources: 0,
      remainingInfluence: 0,
      totalInfluence: 0,
      controlledSystems: 0,
      controlledPlanets: 0,
      availableProductionCapacity: 0,
      totalProductionCapacity: 0,
      productionCapacity: 0,
    });
  });

  it("identifies ships correctly", () => {
    expect(isShip("carrier")).toBe(true);
    expect(isShip("sol_carrier2")).toBe(true);
    expect(isShip("fighter")).toBe(true);
    expect(isShip("cruiser")).toBe(true);
    expect(isShip("destroyer")).toBe(true);
    expect(isShip("dreadnought")).toBe(true);
    expect(isShip("flagship")).toBe(true);
    expect(isShip("warsun")).toBe(true);

    expect(isShip("infantry")).toBe(false);
    expect(isShip("mech")).toBe(false);
    expect(isShip("pds")).toBe(false);
    expect(isShip("space_dock")).toBe(false);
    expect(isShip("spacedock")).toBe(false);
  });

  it("computes resources, influence, controlled planets and systems for single-planet system", () => {
    const board: BoardView = {
      map_tiles: [
        {
          system_id: "1",
          label: "Jord",
          q: 0,
          r: 0,
          planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
        },
      ],
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: false },
          },
          units: [
            { owner: "p1", unit_type: "carrier", damaged: false },
            { owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false },
          ],
        },
      },
    };

    const stats = computePlayerStats(basePlayer, board);
    expect(stats.controlledPlanets).toBe(1);
    expect(stats.controlledSystems).toBe(1);
    expect(stats.remainingResources).toBe(4);
    expect(stats.totalResources).toBe(4);
    expect(stats.remainingInfluence).toBe(2);
    expect(stats.totalInfluence).toBe(2);
    // Space Dock I on Jord (res 4): 4 + 2 = 6
    expect(stats.productionCapacity).toBe(6);
  });

  it("accounts for exhausted planets and attachments", () => {
    const board: BoardView = {
      map_tiles: [
        {
          system_id: "1",
          label: "Jord",
          q: 0,
          r: 0,
          planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
        },
        {
          system_id: "2",
          label: "Mining System",
          q: 1,
          r: 0,
          planets: [{ id: "abyz", label: "Abyz", resources: 3, influence: 0 }],
        },
      ],
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: true },
          },
          units: [{ owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false }],
        },
        "2": {
          system_id: "2",
          command_tokens: [],
          planets: {
            abyz: {
              planet_id: "abyz",
              controlled_by: "p1",
              exhausted: false,
              attachments: ["nanoforge"],
            },
          },
          units: [],
        },
      },
    };

    const stats = computePlayerStats(basePlayer, board);
    expect(stats.controlledPlanets).toBe(2);
    expect(stats.controlledSystems).toBe(2);
    // Abyz has 3 base res + 2 from nanoforge = 5 res. Ready: 5.
    // Jord has 4 base res. Exhausted, so 0 ready, 4 total.
    // Total resources = 4 + 5 = 9.
    // Remaining resources = 5.
    expect(stats.totalResources).toBe(9);
    expect(stats.remainingResources).toBe(5);

    // Abyz has 0 base inf + 2 from nanoforge = 2 inf. Ready: 2.
    // Jord has 2 base inf. Exhausted, so 0 ready, 2 total.
    // Total influence = 2 + 2 = 4.
    // Remaining influence = 2.
    expect(stats.totalInfluence).toBe(4);
    expect(stats.remainingInfluence).toBe(2);

    // Production on Jord: 4 printed + 2 = 6 (exhaustion does not reduce dock capacity)
    expect(stats.productionCapacity).toBe(6);
  });

  it("handles Space Dock II technology upgrade", () => {
    const upgradedPlayer: PlayerView = {
      ...basePlayer,
      technologies: ["sd2"],
    };

    const board: BoardView = {
      map_tiles: [
        {
          system_id: "1",
          label: "Jord",
          q: 0,
          r: 0,
          planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
        },
      ],
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: false },
          },
          units: [{ owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false }],
        },
      },
    };

    const stats = computePlayerStats(upgradedPlayer, board);
    // Space Dock II: 4 res + 4 = 8
    expect(stats.productionCapacity).toBe(8);
  });

  it("blocks system control if enemy ships are present", () => {
    const board: BoardView = {
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: false },
          },
          units: [
            { owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false },
            { owner: "p2", unit_type: "cruiser", damaged: false }, // Enemy ship!
          ],
        },
      },
    };

    const stats = computePlayerStats(basePlayer, board);
    expect(stats.controlledPlanets).toBe(1);
    expect(stats.controlledSystems).toBe(0); // Blocked by p2 cruiser
  });

  it("controls empty system if player is the only one with ships", () => {
    const board: BoardView = {
      systems: {
        empty_1: {
          system_id: "empty_1",
          command_tokens: [],
          planets: {},
          units: [{ owner: "p1", unit_type: "carrier", damaged: false }],
        },
        empty_2: {
          system_id: "empty_2",
          command_tokens: [],
          planets: {},
          units: [], // No ships -> not controlled
        },
      },
    };

    const stats = computePlayerStats(basePlayer, board);
    expect(stats.controlledSystems).toBe(1);
  });

  it("does not control multi-planet system if some planets are uncolonized or controlled by others", () => {
    const board: BoardView = {
      systems: {
        multi: {
          system_id: "multi",
          command_tokens: [],
          planets: {
            bereg: { planet_id: "bereg", controlled_by: "p1", exhausted: false },
            lirtaiv: { planet_id: "lirtaiv", controlled_by: "p2", exhausted: false },
          },
          units: [],
        },
      },
    };

    const stats = computePlayerStats(basePlayer, board);
    expect(stats.controlledPlanets).toBe(1);
    expect(stats.controlledSystems).toBe(0);
  });

  it("computes production capacity for Saar and Arborec units", () => {
    const saarPlayer: PlayerView = {
      ...basePlayer,
      id: "saar_player",
      faction: "Clan of Saar",
    };

    const saarBoard: BoardView = {
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {},
          units: [
            { owner: "saar_player", unit_type: "space_dock", damaged: false }, // Floating Factory I = 5
          ],
        },
      },
    };

    const saarStats = computePlayerStats(saarPlayer, saarBoard);
    expect(saarStats.productionCapacity).toBe(5);

    const arborecPlayer: PlayerView = {
      ...basePlayer,
      id: "arborec_player",
      faction: "Arborec",
      technologies: ["lw2"],
    };

    const arborecBoard: BoardView = {
      systems: {
        "1": {
          system_id: "1",
          command_tokens: [],
          planets: {},
          units: [
            { owner: "arborec_player", unit_type: "infantry", damaged: false }, // Letani Warrior II = 2
            { owner: "arborec_player", unit_type: "infantry", damaged: false }, // Letani Warrior II = 2
            { owner: "arborec_player", unit_type: "mech", damaged: false }, // Letani Behemoth = 2
          ],
        },
      },
    };

    const arborecStats = computePlayerStats(arborecPlayer, arborecBoard);
    expect(arborecStats.productionCapacity).toBe(6);
  });

  it("counts 0 available production capacity in activated systems containing player command token", () => {
    const board: BoardView = {
      map_tiles: [
        {
          system_id: "1",
          label: "Jord",
          q: 0,
          r: 0,
          planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
        },
        {
          system_id: "2",
          label: "Abyz",
          q: 1,
          r: 0,
          planets: [{ id: "abyz", label: "Abyz", resources: 3, influence: 0 }],
        },
      ],
      systems: {
        "1": {
          system_id: "1",
          command_tokens: ["p1"], // Activated by p1!
          planets: {
            jord: { planet_id: "jord", controlled_by: "p1", exhausted: false },
          },
          units: [
            { owner: "p1", unit_type: "space_dock", planet: "jord", damaged: false }, // 4 + 2 = 6
          ],
        },
        "2": {
          system_id: "2",
          command_tokens: ["p2"], // Activated by other player, not p1
          planets: {
            abyz: { planet_id: "abyz", controlled_by: "p1", exhausted: false },
          },
          units: [
            { owner: "p1", unit_type: "space_dock", planet: "abyz", damaged: false }, // 3 + 2 = 5
          ],
        },
      },
    };

    const stats = computePlayerStats(basePlayer, board);
    // Total production capacity: 6 + 5 = 11
    expect(stats.totalProductionCapacity).toBe(11);
    // Available production capacity: System 1 is activated by p1 (0), System 2 is not activated by p1 (5)
    expect(stats.availableProductionCapacity).toBe(5);
  });
});
