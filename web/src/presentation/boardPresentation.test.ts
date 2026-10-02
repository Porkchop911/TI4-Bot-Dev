import { describe, it, expect } from "vitest";
import {
  getPlayerColor,
  deriveOwnershipPalette,
  deriveHexGeometry,
  deriveAnomalyVisual,
  deriveWormholeVisual,
  deriveActorTargetHighlights,
  deriveSelectedSystemDetails,
  buildBoardPresentationModel,
  PLAYER_PALETTE,
  NEUTRAL_COLOR,
  UNASSIGNED_COLOR,
} from "./boardPresentation.ts";
import { BoardView, PendingChoiceDto, PlayerView } from "../protocol/types.ts";

describe("boardPresentation Presentation Model", () => {
  describe("Ownership Colors & Palettes", () => {
    it("assigns colors strictly by projected seating order position, not seat string patterns", () => {
      const seatingOrder = ["custom-alpha", "player_99", "uuid-3a7b-44", "seat_0"];

      expect(getPlayerColor("custom-alpha", seatingOrder)).toBe(PLAYER_PALETTE[0]);
      expect(getPlayerColor("player_99", seatingOrder)).toBe(PLAYER_PALETTE[1]);
      expect(getPlayerColor("uuid-3a7b-44", seatingOrder)).toBe(PLAYER_PALETTE[2]);
      // Even though the seat is named 'seat_0', its index is 3 so it gets color 3
      expect(getPlayerColor("seat_0", seatingOrder)).toBe(PLAYER_PALETTE[3]);

      // Unowned / empty
      expect(getPlayerColor(null, seatingOrder)).toBe(NEUTRAL_COLOR);
      expect(getPlayerColor(undefined, seatingOrder)).toBe(NEUTRAL_COLOR);
      expect(getPlayerColor("", seatingOrder)).toBe(NEUTRAL_COLOR);

      // Not in seating order
      expect(getPlayerColor("ghost-seat", seatingOrder)).toBe(UNASSIGNED_COLOR);
    });

    it("builds ownership palette mapping every seat to a distinct style token", () => {
      const seatingOrder = ["seat_a", "seat_b"];
      const players: PlayerView[] = [
        {
          id: "seat_a",
          faction: "sol",
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
        },
      ];

      const palette = deriveOwnershipPalette(seatingOrder, players);
      expect(palette.get("seat_a")?.color).toBe(PLAYER_PALETTE[0]);
      expect(palette.get("seat_a")?.faction).toBe("sol");
      expect(palette.get("seat_b")?.color).toBe(PLAYER_PALETTE[1]);
    });
  });

  describe("Hex Geometry & Tile Visuals", () => {
    it("lays fracture and nexus tiles below the actual galaxy without overlaps", () => {
      const board = {
        systems: {},
        map_tiles: [
          { system_id: "18", label: "Center", q: 0, r: 0 },
          { system_id: "top", label: "South", q: 0, r: 4 },
          { system_id: "f0", label: "Fracture A", q: 0, r: 0, special_area: "fracture" },
          { system_id: "f1", label: "Fracture B", q: 1, r: 0, special_area: "fracture" },
          { system_id: "n", label: "Nexus", q: 0, r: 0, special_area: "nexus" },
        ],
      };
      const model = buildBoardPresentationModel(board, []);
      const tiles = model.tiles;
      const south = tiles.find((tile) => tile.systemId === "top")!;
      const f0 = tiles.find((tile) => tile.systemId === "f0")!;
      const f1 = tiles.find((tile) => tile.systemId === "f1")!;
      const nexus = tiles.find((tile) => tile.systemId === "n")!;
      expect(f0.center.y - south.center.y).toBeGreaterThan(140);
      expect(f1.center.x - f0.center.x).toBeGreaterThan(140);
      expect(f0.center.x - nexus.center.x).toBeGreaterThan(140);
      const [left, top, width, height] = model.viewBox.split(" ").map(Number);
      expect(left).toBeLessThan(nexus.center.x - 70);
      expect(top + height).toBeGreaterThan(f0.center.y + 70);
      expect(left + width).toBeGreaterThan(f1.center.x + 70);
    });
    it("computes center and hexagon points for normal axial coordinates", () => {
      const geo = deriveHexGeometry(0, 0);
      expect(geo.center).toEqual({ x: 0, y: 0 });
      expect(geo.points).toContain("70.0");

      const geoOffset = deriveHexGeometry(1, 0);
      expect(geoOffset.center).toEqual({ x: 150, y: 0 });
    });

    it("derives anomaly and wormhole styling from structured attributes", () => {
      expect(deriveAnomalyVisual(["Supernova"])?.label).toBe("SUPERNOVA");
      expect(deriveAnomalyVisual(["gravity rift"])?.color).toBe("#38235f");
      expect(deriveAnomalyVisual([])).toBeNull();

      expect(deriveWormholeVisual("Alpha")).toEqual({
        kind: "Alpha",
        symbol: "α",
        color: "#38bdf8",
      });
      expect(deriveWormholeVisual("beta")).toEqual({ kind: "beta", symbol: "β", color: "#f43f5e" });
    });
  });

  describe("Actor-Visible Target Highlighting (Zero Regexes)", () => {
    const mockPendingChoice: PendingChoiceDto = {
      nonce: "nonce_123",
      actor: "seat_a",
      prompt: "Activate a system or produce units",
      options: [
        {
          id: "opt_activate_18",
          kind: "activate",
          label: "Activate Mecatol Rex (#18)",
          payload: { system: "18" },
        },
        {
          id: "opt_move_to_34",
          kind: "move",
          label: "Move Cruiser to #34",
          payload: { from: "18", to: "34", unit: "cruiser" },
        },
        {
          id: "opt_invade_mecatol",
          kind: "commit_ground_forces",
          label: "Land 2 Infantry on Mecatol Rex",
          payload: { system: "18", planet: "mecatol_rex", unit: "infantry" },
        },
      ],
      context: {
        version: 1,
        actor: "seat_a",
        subtype: "tactical_action",
        target: { System: "18" },
      },
    };

    it("derives target highlights from structured payloads for the active actor", () => {
      const highlights = deriveActorTargetHighlights(mockPendingChoice, "seat_a");

      expect(highlights.hasActiveTargets).toBe(true);
      expect(highlights.contextSubjectSystemId).toBe("18");
      expect(highlights.targetableSystemIds.has("18")).toBe(true);
      expect(highlights.targetableSystemIds.has("34")).toBe(true);
      expect(highlights.targetablePlanetIds.has("mecatol_rex")).toBe(true);

      expect(highlights.systemOptionMap.get("18")).toContain("opt_activate_18");
      expect(highlights.systemOptionMap.get("34")).toContain("opt_move_to_34");
      expect(highlights.planetOptionMap.get("mecatol_rex")).toContain("opt_invade_mecatol");
    });

    it("strictly redacts target highlights when viewer is an opponent or spectator", () => {
      const opponentHighlights = deriveActorTargetHighlights(mockPendingChoice, "seat_b");
      expect(opponentHighlights.hasActiveTargets).toBe(false);
      expect(opponentHighlights.targetableSystemIds.size).toBe(0);
      expect(opponentHighlights.contextSubjectSystemId).toBeNull();

      const spectatorHighlights = deriveActorTargetHighlights(mockPendingChoice, null);
      expect(spectatorHighlights.hasActiveTargets).toBe(false);
      expect(spectatorHighlights.targetableSystemIds.size).toBe(0);
    });

    it("handles activate options where system is in kind and payload", () => {
      const activateChoice: PendingChoiceDto = {
        nonce: "nonce_act",
        actor: "seat_a",
        prompt: "Activate a system",
        options: [
          {
            id: "55",
            kind: "activate",
            label: "Activate System 55",
            payload: { system: "55" },
          },
        ],
      };

      const highlights = deriveActorTargetHighlights(activateChoice, "seat_a");
      expect(highlights.targetableSystemIds.has("55")).toBe(true);
      expect(highlights.systemOptionMap.get("55")).toEqual(["55"]);
    });
  });

  describe("Selected System Details Inspector", () => {
    const mockBoard: BoardView = {
      active_system: "18",
      systems: {
        "18": {
          system_id: "18",
          command_tokens: ["seat_a"],
          planets: {
            mecatol_rex: {
              planet_id: "mecatol_rex",
              controlled_by: "seat_a",
              exhausted: false,
              attachments: ["custodians_token"],
            },
          },
          units: [
            { unit_type: "carrier", owner: "seat_a", planet: null, damaged: false },
            { unit_type: "infantry", owner: "seat_a", planet: "mecatol_rex", damaged: false },
          ],
        },
      },
      map_tiles: [
        {
          system_id: "18",
          label: "Mecatol Rex",
          q: 0,
          r: 0,
          planets: [
            {
              id: "mecatol_rex",
              label: "Mecatol Rex",
              resources: 1,
              influence: 6,
              traits: ["cultural"],
              tech_specialties: [],
              legendary: true,
            },
          ],
        },
      ],
    };

    it("extracts complete structured system details including planets, units, and available actions", () => {
      const pendingChoice: PendingChoiceDto = {
        nonce: "n1",
        actor: "seat_a",
        prompt: "Produce units",
        options: [
          {
            id: "prod_fighter",
            kind: "produce_unit",
            label: "Produce 2 Fighters",
            payload: { system: "18", unit: "fighter" },
          },
        ],
      };

      const details = deriveSelectedSystemDetails(
        "18",
        mockBoard,
        ["seat_a", "seat_b"],
        [],
        pendingChoice,
        "seat_a",
      );

      expect(details).not.toBeNull();
      expect(details?.systemId).toBe("18");
      expect(details?.label).toBe("Mecatol Rex");
      expect(details?.isActiveSystem).toBe(true);

      // Planets
      expect(details?.planets.length).toBe(1);
      expect(details?.planets[0].label).toBe("Mecatol Rex");
      expect(details?.planets[0].resources).toBe(1);
      expect(details?.planets[0].influence).toBe(6);
      expect(details?.planets[0].controlledBy).toBe("seat_a");
      expect(details?.planets[0].attachments).toEqual(["custodians_token"]);

      // Space vs Planet units
      expect(details?.spaceUnits.length).toBe(1);
      expect(details?.spaceUnits[0].unitType).toBe("carrier");
      expect(details?.planetUnits["mecatol_rex"].length).toBe(1);
      expect(details?.planetUnits["mecatol_rex"][0].unitType).toBe("infantry");

      // Command tokens
      expect(details?.commandTokens).toEqual([{ owner: "seat_a", color: PLAYER_PALETTE[0] }]);

      // Available actions
      expect(details?.availableActions).toEqual([
        { optionId: "prod_fighter", label: "Produce 2 Fighters", kind: "produce_unit" },
      ]);
    });
  });

  describe("Full BoardPresentationModel Assembly", () => {
    it("integrates layout, colors, tiles, and target state", () => {
      const board: BoardView = {
        systems: {
          "18": { system_id: "18", command_tokens: [], planets: {}, units: [] },
        },
        map_tiles: [{ system_id: "18", label: "Mecatol Rex", q: 0, r: 0 }],
      };

      const model = buildBoardPresentationModel(board, ["seat_1"], [], null, "seat_1", "18");
      expect(model.tiles.length).toBe(1);
      expect(model.tiles[0].systemId).toBe("18");
      expect(model.selectedSystem?.systemId).toBe("18");
      expect(model.ownershipMap.has("seat_1")).toBe(true);
    });

    it("derives isActivationMode and movementVectors from tactical choices", () => {
      const board: BoardView = {
        active_system: "18",
        systems: {
          "18": { system_id: "18", command_tokens: [], planets: {}, units: [] },
          "24": { system_id: "24", command_tokens: [], planets: {}, units: [] },
        },
        map_tiles: [
          { system_id: "18", label: "Mecatol Rex", q: 0, r: 0 },
          { system_id: "24", label: "Moll Primus", q: 1, r: 0 },
        ],
      };

      // 1. Activation Mode
      const activationChoice: PendingChoiceDto = {
        nonce: "n_act",
        actor: "seat_1",
        prompt: "activate a system",
        context: { subtype: "activate_system" },
        options: [{ id: "18", label: "Mecatol Rex", kind: "activate" }],
      };
      const actModel = buildBoardPresentationModel(
        board,
        ["seat_1"],
        [],
        activationChoice,
        "seat_1",
      );
      expect(actModel.targets.isActivationMode).toBe(true);

      // 2. Movement Vectors
      const moveChoice: PendingChoiceDto = {
        nonce: "n_move",
        actor: "seat_1",
        prompt: "movement",
        context: {
          subtype: "movement_step",
          target: { System: "18" },
        },
        options: [
          {
            id: "move|24|0",
            label: "Cruiser",
            kind: "move",
            payload: { origin: "24", unit: "cruiser" },
          },
          {
            id: "move|24|1",
            label: "Carrier",
            kind: "move",
            payload: { origin: "24", unit: "carrier" },
          },
          { id: "done_moving", label: "Finish", kind: "decline" },
        ],
      };

      const moveModel = buildBoardPresentationModel(board, ["seat_1"], [], moveChoice, "seat_1");
      expect(moveModel.targets.movementVectors).toHaveLength(1);
      const vec = moveModel.targets.movementVectors[0];
      expect(vec.fromSystemId).toBe("24");
      expect(vec.toSystemId).toBe("18");
      expect(vec.unitCount).toBe(2);
      expect(vec.fromCenter.x).toBe(150);
      expect(vec.toCenter.x).toBe(0);
    });
  });
});
