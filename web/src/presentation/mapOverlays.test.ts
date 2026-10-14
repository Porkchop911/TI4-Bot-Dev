import { describe, it, expect } from "vitest";
import {
  getExpectedSpaceHits,
  getExpectedGroundHits,
  hasSustainDamage,
  computeTileEconomy,
  computeTileSpaceCombat,
  computeTileGroundCombat,
  computeTileTechBenefits,
} from "./mapOverlays.ts";
import { TilePresentation } from "./boardPresentation.ts";

function createMockTile(overrides: Partial<TilePresentation> = {}): TilePresentation {
  return {
    systemId: "1",
    label: "Test System",
    q: 0,
    r: 0,
    center: { x: 100, y: 100 },
    points: "",
    innerPoints: "",
    fillColor: "#000",
    strokeColor: "#333",
    strokeWidth: 1.5,
    anomalies: [],
    wormholes: [],
    planets: [],
    units: [],
    totalUnits: 0,
    commandTokens: [],
    isCandidateTarget: false,
    isContextSubject: false,
    associatedOptionIds: [],
    ...overrides,
  };
}

describe("mapOverlays", () => {
  describe("getExpectedSpaceHits", () => {
    it("returns correct standard base hits for ships", () => {
      expect(getExpectedSpaceHits("warsun")).toBe(2.4);
      expect(getExpectedSpaceHits("flagship")).toBe(1.2);
      expect(getExpectedSpaceHits("dreadnought")).toBe(0.6);
      expect(getExpectedSpaceHits("cruiser")).toBe(0.4);
      expect(getExpectedSpaceHits("destroyer")).toBe(0.2);
      expect(getExpectedSpaceHits("fighter")).toBe(0.2);
      expect(getExpectedSpaceHits("carrier")).toBe(0.2);
    });

    it("handles faction variants", () => {
      expect(getExpectedSpaceHits("sol_carrier2")).toBe(0.2);
      expect(getExpectedSpaceHits("superdreadnought_1")).toBe(0.6);
      expect(getExpectedSpaceHits("superdreadnought_2")).toBe(0.7);
      expect(getExpectedSpaceHits("dreadnought", { faction: "l1z1x", technologies: ["dn2"] })).toBe(
        0.7,
      );
      expect(getExpectedSpaceHits("saturn_engine_1")).toBe(0.4);

      // Argent Strike Wing Alpha: I hits on 8+ (0.3), II hits on 7+ (0.4)
      expect(getExpectedSpaceHits("argent_destroyer")).toBe(0.3);
      expect(getExpectedSpaceHits("argent_destroyer2")).toBe(0.4);
      expect(getExpectedSpaceHits("destroyer", { faction: "argent" })).toBe(0.3);
      expect(getExpectedSpaceHits("destroyer", { faction: "argent", technologies: ["dd2"] })).toBe(
        0.4,
      );

      // Naalu Hybrid Crystal Fighter: I hits on 8+ (0.3), II hits on 7+ (0.4)
      expect(getExpectedSpaceHits("naalu_fighter")).toBe(0.3);
      expect(getExpectedSpaceHits("naalu_fighter2")).toBe(0.4);
      expect(getExpectedSpaceHits("fighter", { faction: "naalu" })).toBe(0.3);
      expect(getExpectedSpaceHits("fighter", { faction: "naalu", technologies: ["ff2"] })).toBe(
        0.4,
      );

      // Faction flagships
      expect(getExpectedSpaceHits("ghost_flagship")).toBe(0.6); // 1 die on 5+
      expect(getExpectedSpaceHits("naalu_flagship")).toBe(0.4); // 2 dice on 9+
      expect(getExpectedSpaceHits("flagship", { faction: "naalu" })).toBe(0.4);
      expect(getExpectedSpaceHits("xxcha_flagship")).toBe(0.8); // 2 dice on 7+
      expect(getExpectedSpaceHits("flagship", { faction: "hacan" })).toBe(0.8);
      expect(getExpectedSpaceHits("flagship", { faction: "sol" })).toBe(1.2); // 2 dice on 5+
      expect(getExpectedSpaceHits("winnu_flagship")).toBe(0);
    });

    it("applies unit upgrade technologies from player context", () => {
      // Cruiser II: hits on 6+ instead of 7+
      expect(getExpectedSpaceHits("cruiser", { technologies: ["cr2"] })).toBe(0.5);
      expect(getExpectedSpaceHits("cruiser2")).toBe(0.5);

      // Destroyer II: hits on 8+ instead of 9+
      expect(getExpectedSpaceHits("destroyer", { technologies: ["dd2"] })).toBe(0.3);
      expect(getExpectedSpaceHits("destroyer2")).toBe(0.3);

      // Fighter II: hits on 8+ instead of 9+
      expect(getExpectedSpaceHits("fighter", { technologies: ["ff2"] })).toBe(0.3);
      expect(getExpectedSpaceHits("fighter2")).toBe(0.3);
    });

    it("applies faction combat modifiers from player context", () => {
      // Sardakk N'orr: +1 to all combat rolls
      expect(getExpectedSpaceHits("cruiser", { faction: "sardakk" })).toBe(0.5);
      expect(getExpectedSpaceHits("warsun", { faction: "sardakk" })).toBe(2.7);
      // Sardakk with Cruiser II: hits on 5+ (6 - 1) -> 0.6
      expect(getExpectedSpaceHits("cruiser", { faction: "sardakk", technologies: ["cr2"] })).toBe(
        0.6,
      );

      // Universities of Jol-Nar: -1 to all combat rolls
      expect(getExpectedSpaceHits("cruiser", { faction: "jolnar" })).toBe(0.3);
      expect(getExpectedSpaceHits("warsun", { faction: "jolnar" })).toBe(2.1);
      // Jol-Nar Flagship (hits on 7+ plus bonus on 9 and 10)
      expect(getExpectedSpaceHits("flagship", { faction: "jolnar" })).toBe(1.6);
    });
  });

  describe("getExpectedGroundHits", () => {
    it("returns correct hits for infantry and mech", () => {
      expect(getExpectedGroundHits("infantry")).toBe(0.3);
      expect(getExpectedGroundHits("mech")).toBe(0.5);
      expect(getExpectedGroundHits("fighter")).toBe(0);
    });

    it("applies Infantry II and faction abilities to ground hits", () => {
      // Infantry II: hits on 7+ instead of 8+
      expect(getExpectedGroundHits("infantry", { technologies: ["inf2"] })).toBe(0.4);
      expect(getExpectedGroundHits("infantry2")).toBe(0.4);

      // Sardakk +1 on ground
      expect(getExpectedGroundHits("infantry", { faction: "sardakk" })).toBe(0.4);
      expect(getExpectedGroundHits("mech", { faction: "sardakk" })).toBe(0.6);

      // Jol-Nar -1 on ground
      expect(getExpectedGroundHits("infantry", { faction: "jolnar" })).toBe(0.2);
      expect(getExpectedGroundHits("mech", { faction: "jolnar" })).toBe(0.4);

      // Sol Spec Ops
      expect(getExpectedGroundHits("infantry", { faction: "sol" })).toBe(0.4);
      expect(
        getExpectedGroundHits("infantry", { faction: "sol", technologies: ["spec_ops_2"] }),
      ).toBe(0.5);

      // Naaz-Rokha Mech (Eidolon): rolls 2 dice hitting on 6+ (1.0 hit)
      expect(getExpectedGroundHits("naaz_mech")).toBe(1.0);
      expect(getExpectedGroundHits("mech", { faction: "naaz" })).toBe(1.0);

      // Titans Hel-Titan: PDS participating in ground combat
      expect(getExpectedGroundHits("titans_pds")).toBe(0.4);
      expect(getExpectedGroundHits("titans_pds2")).toBe(0.5);
      expect(getExpectedGroundHits("pds", { faction: "titans", technologies: ["pds2"] })).toBe(0.5);
    });
  });

  describe("hasSustainDamage", () => {
    it("identifies sustain damage units", () => {
      expect(hasSustainDamage("warsun")).toBe(true);
      expect(hasSustainDamage("flagship")).toBe(true);
      expect(hasSustainDamage("dreadnought")).toBe(true);
      expect(hasSustainDamage("mech")).toBe(true);
      expect(hasSustainDamage("cruiser")).toBe(false);
      expect(hasSustainDamage("infantry")).toBe(false);
    });

    it("recognizes upgraded units with sustain damage", () => {
      expect(hasSustainDamage("saturn_engine_2")).toBe(true);
      expect(
        hasSustainDamage("cruiser", { faction: "titans", technologies: ["saturn_engine_2"] }),
      ).toBe(true);
      expect(hasSustainDamage("sol_carrier2")).toBe(true);
      expect(hasSustainDamage("titans_pds")).toBe(true);
      expect(hasSustainDamage("titans_pds2")).toBe(true);
    });
  });

  describe("computeTileEconomy", () => {
    it("aggregates ready vs exhausted resources and influence", () => {
      const tile = createMockTile({
        planets: [
          {
            id: "planet_a",
            label: "Planet A",
            resources: 3,
            influence: 1,
            traits: [],
            techSpecialties: [],
            legendary: false,
            controlledBy: "p1",
            controllerColor: "#f00",
            exhausted: false,
            attachments: [],
            isCandidateTarget: false,
            isContextSubject: false,
            associatedOptionIds: [],
          },
          {
            id: "planet_b",
            label: "Planet B",
            resources: 1,
            influence: 2,
            traits: [],
            techSpecialties: [],
            legendary: false,
            controlledBy: "p1",
            controllerColor: "#f00",
            exhausted: true,
            attachments: [],
            isCandidateTarget: false,
            isContextSubject: false,
            associatedOptionIds: [],
          },
        ],
      });

      const eco = computeTileEconomy(tile);
      expect(eco.hasPlanets).toBe(true);
      expect(eco.totalResources).toBe(4);
      expect(eco.totalInfluence).toBe(3);
      expect(eco.readyResources).toBe(3);
      expect(eco.readyInfluence).toBe(1);
      expect(eco.exhaustedResources).toBe(1);
      expect(eco.exhaustedInfluence).toBe(2);
      expect(eco.planets).toHaveLength(2);
    });
  });

  describe("computeTileSpaceCombat", () => {
    it("calculates fleet size, avg hits and sustain damage per owner in space", () => {
      const tile = createMockTile({
        units: [
          { unitType: "dreadnought", owner: "p1", ownerColor: "#3b82f6", damaged: false },
          { unitType: "cruiser", owner: "p1", ownerColor: "#3b82f6", damaged: false },
          { unitType: "fighter", owner: "p1", ownerColor: "#3b82f6", damaged: false },
          {
            unitType: "infantry",
            owner: "p1",
            ownerColor: "#3b82f6",
            planet: "planet_1",
            damaged: false,
          }, // on planet
          { unitType: "destroyer", owner: "p2", ownerColor: "#ef4444", damaged: false },
        ],
      });

      const combat = computeTileSpaceCombat(tile);
      expect(combat.hasCombatUnits).toBe(true);
      expect(combat.fleets).toHaveLength(2);

      const p1Fleet = combat.fleets.find((f) => f.owner === "p1")!;
      expect(p1Fleet.totalUnits).toBe(3); // dreadnought + cruiser + fighter (infantry ignored because on planet)
      expect(p1Fleet.capitalShipCount).toBe(2);
      expect(p1Fleet.fighterCount).toBe(1);
      expect(p1Fleet.sustainCount).toBe(1); // 1 undamaged dreadnought
      // dreadnought 0.6 + cruiser 0.4 + fighter 0.2 = 1.2
      expect(p1Fleet.avgHits).toBe(1.2);

      const p2Fleet = combat.fleets.find((f) => f.owner === "p2")!;
      expect(p2Fleet.totalUnits).toBe(1);
      expect(p2Fleet.avgHits).toBe(0.2);
    });

    it("applies player public context (faction and technologies) to space combat calculations", () => {
      const tile = createMockTile({
        units: [
          // p1 is Sardakk with Cruiser II: dreadnought (0.7) + cruiser (0.6) = 1.3
          { unitType: "dreadnought", owner: "p1", ownerColor: "#f00", damaged: false },
          { unitType: "cruiser", owner: "p1", ownerColor: "#f00", damaged: false },
          // p2 is Jol-Nar: destroyer (0.1)
          { unitType: "destroyer", owner: "p2", ownerColor: "#00f", damaged: false },
        ],
      });

      const players = [
        { id: "p1", faction: "sardakk", technologies: ["cr2"] } as any,
        { id: "p2", faction: "jolnar", technologies: [] } as any,
      ];

      const combat = computeTileSpaceCombat(tile, players);
      const p1Fleet = combat.fleets.find((f) => f.owner === "p1")!;
      expect(p1Fleet.avgHits).toBe(1.3);

      const p2Fleet = combat.fleets.find((f) => f.owner === "p2")!;
      expect(p2Fleet.avgHits).toBe(0.1);
    });

    it("does not count transported infantry or ground units in space combat ship count or combat values", () => {
      const tile = createMockTile({
        units: [
          // Carrier with 2 infantry transported in space (no planet)
          { unitType: "carrier", owner: "p1", ownerColor: "#3b82f6", damaged: false },
          { unitType: "infantry", owner: "p1", ownerColor: "#3b82f6", damaged: false },
          { unitType: "infantry", owner: "p1", ownerColor: "#3b82f6", damaged: false },
          // A mech in space (no planet)
          { unitType: "mech", owner: "p1", ownerColor: "#3b82f6", damaged: false },
        ],
      });

      const combat = computeTileSpaceCombat(tile);
      expect(combat.hasCombatUnits).toBe(true);
      expect(combat.fleets).toHaveLength(1);

      const p1Fleet = combat.fleets[0];
      // Only the 1 carrier can participate in space combat; 2 infantry and 1 mech must NOT be counted
      expect(p1Fleet.totalUnits).toBe(1);
      expect(p1Fleet.capitalShipCount).toBe(1);
      expect(p1Fleet.fighterCount).toBe(0);
      expect(p1Fleet.avgHits).toBe(0.2); // carrier only (0.2 hits), infantry (0.3 each) and mech (0.5) not added
      expect(p1Fleet.sustainCount).toBe(0); // carrier has no sustain; mech sustain not counted in space combat
      expect(p1Fleet.unitCounts["infantry"]).toBeUndefined();
      expect(p1Fleet.unitCounts["mech"]).toBeUndefined();
      expect(p1Fleet.unitCounts["carrier"]).toBe(1);
    });

    it("returns hasCombatUnits false if only infantry/ground forces are in space", () => {
      const tile = createMockTile({
        units: [{ unitType: "infantry", owner: "p1", ownerColor: "#3b82f6", damaged: false }],
      });

      const combat = computeTileSpaceCombat(tile);
      expect(combat.hasCombatUnits).toBe(false);
      expect(combat.fleets).toHaveLength(0);
    });
  });

  describe("computeTileGroundCombat", () => {
    it("aggregates planetary ground units, avg hits, sustain, and PDS", () => {
      const tile = createMockTile({
        planets: [
          {
            id: "planet_1",
            label: "Jord",
            resources: 4,
            influence: 2,
            traits: [],
            techSpecialties: [],
            legendary: false,
            controlledBy: "p1",
            controllerColor: "#3b82f6",
            exhausted: false,
            attachments: [],
            isCandidateTarget: false,
            isContextSubject: false,
            associatedOptionIds: [],
          },
        ],
        units: [
          {
            unitType: "infantry",
            owner: "p1",
            ownerColor: "#3b82f6",
            planet: "planet_1",
            damaged: false,
          },
          {
            unitType: "infantry",
            owner: "p1",
            ownerColor: "#3b82f6",
            planet: "planet_1",
            damaged: false,
          },
          {
            unitType: "mech",
            owner: "p1",
            ownerColor: "#3b82f6",
            planet: "planet_1",
            damaged: false,
          },
          {
            unitType: "pds",
            owner: "p1",
            ownerColor: "#3b82f6",
            planet: "planet_1",
            damaged: false,
          },
          { unitType: "fighter", owner: "p1", ownerColor: "#3b82f6", damaged: false }, // space unit
        ],
      });

      const ground = computeTileGroundCombat(tile);
      expect(ground.hasGroundForces).toBe(true);
      expect(ground.planets).toHaveLength(1);

      const pSummary = ground.planets[0];
      expect(pSummary.totalDefenders).toBe(3); // 2 inf + 1 mech
      expect(pSummary.forces).toHaveLength(1);

      const force = pSummary.forces[0];
      expect(force.infantryCount).toBe(2);
      expect(force.mechCount).toBe(1);
      expect(force.sustainCount).toBe(1); // 1 undamaged mech
      expect(force.pdsCount).toBe(1);
      expect(force.hasPlanetaryShield).toBe(true);
      expect(force.spaceCannonDefenseHits).toBe(0.5);
      // 2 * 0.3 + 1 * 0.5 = 1.1 hits
      expect(force.avgHits).toBe(1.1);
    });

    it("applies player public context (faction and technologies) to ground combat calculations", () => {
      const tile = createMockTile({
        planets: [
          {
            id: "planet_1",
            label: "Tren'Lak",
            resources: 1,
            influence: 0,
            traits: [],
            techSpecialties: [],
            legendary: false,
            controlledBy: "p1",
            controllerColor: "#f00",
            exhausted: false,
            attachments: [],
            isCandidateTarget: false,
            isContextSubject: false,
            associatedOptionIds: [],
          },
        ],
        units: [
          // p1 is Sardakk with Infantry II and PDS II:
          // 2 Infantry II: 2 * 0.5 = 1.0 (7+ base with +1 = 6+ -> 0.5)
          // 1 Mech: 1 * 0.6 = 0.6 (6+ base with +1 = 5+ -> 0.6)
          // Total hits = 1.6
          // 1 PDS II: hits on 5+ with +1 = 4+ -> 0.7 hits
          {
            unitType: "infantry",
            owner: "p1",
            ownerColor: "#f00",
            planet: "planet_1",
            damaged: false,
          },
          {
            unitType: "infantry",
            owner: "p1",
            ownerColor: "#f00",
            planet: "planet_1",
            damaged: false,
          },
          { unitType: "mech", owner: "p1", ownerColor: "#f00", planet: "planet_1", damaged: false },
          { unitType: "pds", owner: "p1", ownerColor: "#f00", planet: "planet_1", damaged: false },
        ],
      });

      const players = [{ id: "p1", faction: "sardakk", technologies: ["inf2", "pds2"] } as any];

      const ground = computeTileGroundCombat(tile, players);
      const force = ground.planets[0].forces[0];
      expect(force.avgHits).toBe(1.6);
      expect(force.spaceCannonDefenseHits).toBe(0.7);
    });
  });

  describe("computeTileTechBenefits", () => {
    it("extracts tech specialties and counts", () => {
      const tile = createMockTile({
        planets: [
          {
            id: "planet_1",
            label: "Wellon",
            resources: 1,
            influence: 2,
            traits: ["industrial"],
            techSpecialties: ["cybernetic"],
            legendary: false,
            controlledBy: "p1",
            controllerColor: "#3b82f6",
            exhausted: false,
            attachments: [],
            isCandidateTarget: false,
            isContextSubject: false,
            associatedOptionIds: [],
          },
        ],
      });

      const tech = computeTileTechBenefits(tile);
      expect(tech.hasTechSpecialties).toBe(true);
      expect(tech.specialtyCounts.cybernetic).toBe(1);
      expect(tech.specialtyCounts.biotic).toBe(0);
      expect(tech.planets[0].specialties).toEqual(["cybernetic"]);
    });

    it("handles uppercase tech specialty strings without crashing", () => {
      const tile = createMockTile({
        planets: [
          {
            id: "rigel_iii",
            label: "Rigel III",
            resources: 1,
            influence: 1,
            traits: ["INDUSTRIAL"],
            // Real backend/init.json data has uppercase "BIOTIC"
            techSpecialties: ["BIOTIC" as any],
            legendary: false,
            controlledBy: null,
            controllerColor: "#64748b",
            exhausted: false,
            attachments: [],
            isCandidateTarget: false,
            isContextSubject: false,
            associatedOptionIds: [],
          },
        ],
      });

      const tech = computeTileTechBenefits(tile);
      expect(tech.hasTechSpecialties).toBe(true);
      expect(tech.specialtyCounts.biotic).toBe(1);
      expect(tech.planets[0].specialties).toEqual(["biotic"]);
    });
  });
});
