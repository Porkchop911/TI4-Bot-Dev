import { TilePresentation, PlacedUnitPresentation, TechSpecialty } from "./boardPresentation.ts";
import { getUnitBaseType } from "../components/UnitIcon.tsx";
import { PlayerView } from "../protocol/types.ts";

export type MapOverlayMode =
  | "none"
  | "economy"
  | "space_combat"
  | "ground_combat"
  | "tech_benefits";

export interface OverlayTileEconomy {
  totalResources: number;
  totalInfluence: number;
  readyResources: number;
  readyInfluence: number;
  exhaustedResources: number;
  exhaustedInfluence: number;
  hasPlanets: boolean;
  planets: Array<{
    id: string;
    label: string;
    resources: number;
    influence: number;
    exhausted: boolean;
    controlledBy: string | null;
    controllerColor: string;
  }>;
}

export interface PlayerFleetCombatSummary {
  owner: string;
  ownerColor: string;
  totalUnits: number;
  capitalShipCount: number;
  fighterCount: number;
  avgHits: number;
  sustainCount: number;
  unitCounts: Record<string, number>;
}

export interface OverlayTileSpaceCombat {
  hasCombatUnits: boolean;
  fleets: PlayerFleetCombatSummary[];
}

export interface PlanetGroundForcesSummary {
  owner: string;
  ownerColor: string;
  infantryCount: number;
  mechCount: number;
  totalUnits: number;
  avgHits: number;
  sustainCount: number;
  pdsCount: number;
  hasPlanetaryShield: boolean;
  spaceCannonDefenseHits: number;
}

export interface PlanetGroundCombatSummary {
  id: string;
  label: string;
  controlledBy: string | null;
  controllerColor: string;
  forces: PlanetGroundForcesSummary[];
  totalDefenders: number;
  hasForces: boolean;
}

export interface OverlayTileGroundCombat {
  hasGroundForces: boolean;
  planets: PlanetGroundCombatSummary[];
}

export interface TechSpecialtyPlanetSummary {
  id: string;
  label: string;
  specialties: TechSpecialty[];
  traits: string[];
  exhausted: boolean;
  controlledBy: string | null;
  controllerColor: string;
}

export interface OverlayTileTechBenefits {
  hasTechSpecialties: boolean;
  planets: TechSpecialtyPlanetSummary[];
  specialtyCounts: Record<TechSpecialty, number>;
}

export interface CombatPublicContext {
  faction?: string | null;
  technologies?: readonly string[] | null;
}

/**
 * Gets a faction's inherent combat roll modifier (e.g. Sardakk Unrelenting +1, Jol-Nar Fragile -1).
 */
export function getFactionCombatModifier(faction?: string | null): number {
  if (!faction) return 0;
  const norm = faction.toLowerCase().trim();
  if (norm.includes("sardakk")) return 1;
  if (norm.includes("jolnar") || norm.includes("jol_nar")) return -1;
  return 0;
}

/**
 * Checks if a unit is upgraded, either via explicit unit type ID or through public player technologies.
 */
export function isUnitUpgraded(
  unitType: string,
  baseType: string,
  technologies?: readonly string[] | null,
): boolean {
  const normType = unitType.toLowerCase().replace(/[-_\s]/g, "");
  // Check if unit type string itself indicates an upgrade
  if (normType.includes("2") || normType.includes("ii")) {
    return true;
  }
  if (
    normType.includes("saturnengine2") ||
    normType.includes("strikewingalpha2") ||
    normType.includes("specops2") ||
    normType.includes("letaniwarrior2") ||
    normType.includes("crimsonlegionnaire2") ||
    normType.includes("superdreadnought2") ||
    normType.includes("exotrireme2") ||
    normType.includes("hybridcrystalfighter2") ||
    normType.includes("advancedcarrier2") ||
    normType.includes("solcarrier2")
  ) {
    return true;
  }

  // Check if player has researched the corresponding upgrade tech
  if (technologies && technologies.length > 0) {
    const techSet = new Set(technologies.map((t) => t.toLowerCase().replace(/[-_\s]/g, "")));
    switch (baseType) {
      case "cruiser":
        return techSet.has("cr2") || techSet.has("cruiser2") || techSet.has("saturnengine2");
      case "destroyer":
        return techSet.has("dd2") || techSet.has("destroyer2") || techSet.has("strikewingalpha2");
      case "fighter":
        return (
          techSet.has("ft2") ||
          techSet.has("ff2") ||
          techSet.has("fighter2") ||
          techSet.has("hybridcrystalfighter2")
        );
      case "infantry":
        return (
          techSet.has("inf2") ||
          techSet.has("infantry2") ||
          techSet.has("specops2") ||
          techSet.has("letaniwarrior2") ||
          techSet.has("crimsonlegionnaire2")
        );
      case "dreadnought":
        return (
          techSet.has("dn2") ||
          techSet.has("dreadnought2") ||
          techSet.has("superdreadnought2") ||
          techSet.has("exotrireme2")
        );
      case "carrier":
        return (
          techSet.has("cv2") ||
          techSet.has("carrier2") ||
          techSet.has("advancedcarrier2") ||
          techSet.has("solcarrier2")
        );
      case "warsun":
        return techSet.has("ws2") || techSet.has("warsun2") || techSet.has("pws2");
      case "pds":
        return techSet.has("pds2") || techSet.has("heltitan2");
      default:
        return false;
    }
  }
  return false;
}

function calculateHitChance(hitsOn: number, modifier: number): number {
  const needed = hitsOn - modifier;
  return Math.max(0, Math.min(1, (11 - needed) / 10));
}

/**
 * Expected combat hits per round for space combat units.
 * Accounts for public information: unit upgrades and faction abilities (Sardakk, Jol-Nar, flagships).
 */
export function getExpectedSpaceHits(
  unitType: string,
  context?: CombatPublicContext | PlayerView | null,
): number {
  const base = getUnitBaseType(unitType);
  const faction = context?.faction;
  const technologies = context?.technologies;
  const shift = getFactionCombatModifier(faction);
  const upgraded = isUnitUpgraded(unitType, base, technologies);
  const normType = unitType.toLowerCase().replace(/[-_\s]/g, "");

  switch (base) {
    case "warsun": {
      // 3 dice hitting on 3+
      const singleDie = calculateHitChance(3, shift);
      return Math.round(3 * singleDie * 100) / 100;
    }
    case "flagship": {
      // Jol-Nar Flagship (J.N.S. Hylarim): 2 dice on 6+, 9 or 10 produces 2 additional hits
      if (normType.includes("jolnar") || normType.includes("hylarim") || faction === "jolnar") {
        const baseDie = calculateHitChance(6, shift);
        return Math.round(2 * (baseDie + 0.4) * 100) / 100;
      }
      // Ghost of Creuss Flagship (Hil Colish): 1 die on 5+
      if (normType.includes("ghost") || normType.includes("hilcolish") || faction === "ghost") {
        return Math.round(calculateHitChance(5, shift) * 100) / 100;
      }
      // Bastion Flagship: 1 die on 9+
      if (normType.includes("bastion") || faction === "bastion") {
        return Math.round(calculateHitChance(9, shift) * 100) / 100;
      }
      // Obsidian Flagship: 3 dice on 5+
      if (normType.includes("obsidian") || faction === "obsidian") {
        return Math.round(3 * calculateHitChance(5, shift) * 100) / 100;
      }
      // Ralnel Flagship: 2 dice on 8+
      if (normType.includes("ralnel") || faction === "ralnel") {
        return Math.round(2 * calculateHitChance(8, shift) * 100) / 100;
      }
      // Sardakk Flagship (C'Morran N'orr): 2 dice on 6+
      if (normType.includes("sardakk") || faction === "sardakk") {
        return Math.round(2 * calculateHitChance(6, shift) * 100) / 100;
      }
      // Winnu Flagship: 0 combat dice baseline (rolls dice equal to opponent non-fighter ships)
      if (normType.includes("winnu") || faction === "winnu") {
        return 0;
      }
      // Flagships hitting on 9+ (2 dice): Naalu, Naaz-Rokha, Nekro, Yin
      if (
        normType.includes("naalu") ||
        normType.includes("naaz") ||
        normType.includes("nekro") ||
        normType.includes("yin") ||
        faction === "naalu" ||
        faction === "naaz" ||
        faction === "nekro" ||
        faction === "yin"
      ) {
        return Math.round(2 * calculateHitChance(9, shift) * 100) / 100;
      }
      // Flagships hitting on 7+ (2 dice): Arborec, Argent, Hacan, Mentak, Nomad I, Titans, Xxcha
      if (
        normType.includes("arborec") ||
        normType.includes("argent") ||
        normType.includes("hacan") ||
        normType.includes("mentak") ||
        normType.includes("titans") ||
        normType.includes("xxcha") ||
        faction === "arborec" ||
        faction === "argent" ||
        faction === "hacan" ||
        faction === "mentak" ||
        faction === "titans" ||
        faction === "xxcha" ||
        (faction === "nomad" && !upgraded && !normType.includes("nomadflagship2"))
      ) {
        return Math.round(2 * calculateHitChance(7, shift) * 100) / 100;
      }
      // Flagships hitting on 5+ (2 dice): Cabal, Crimson, Empyrean, Firmament, L1Z1X, Letnev,
      // Mahact, Muaat, Saar, Sol, Yssaril, Nomad II (Memoria II), Cavalry II, and generic default
      const singleDie = calculateHitChance(5, shift);
      return Math.round(2 * singleDie * 100) / 100;
    }
    case "dreadnought": {
      // L1Z1X Super Dreadnought II hits on 4+ (1 die) instead of 5+
      const isL1Z1X =
        normType.includes("l1z1x") || normType.includes("superdreadnought") || faction === "l1z1x";
      const target = isL1Z1X && upgraded ? 4 : 5;
      return Math.round(calculateHitChance(target, shift) * 100) / 100;
    }
    case "cruiser": {
      // 1 die hitting on 7+ (Cruiser I) or 6+ (Cruiser II)
      const target = upgraded ? 6 : 7;
      return Math.round(calculateHitChance(target, shift) * 100) / 100;
    }
    case "destroyer": {
      // Argent Strike Wing Alpha and Crimson Destroyer: I hits on 8+, II hits on 7+
      const isArgentOrCrimson =
        normType.includes("argent") ||
        normType.includes("strikewingalpha") ||
        normType.includes("crimson") ||
        faction === "argent" ||
        faction === "crimson";
      const target = isArgentOrCrimson ? (upgraded ? 7 : 8) : upgraded ? 8 : 9;
      return Math.round(calculateHitChance(target, shift) * 100) / 100;
    }
    case "fighter": {
      // Naalu Hybrid Crystal Fighter: I hits on 8+, II hits on 7+
      const isNaalu =
        normType.includes("naalu") ||
        normType.includes("hybridcrystalfighter") ||
        faction === "naalu";
      const target = isNaalu ? (upgraded ? 7 : 8) : upgraded ? 8 : 9;
      return Math.round(calculateHitChance(target, shift) * 100) / 100;
    }
    case "carrier": {
      // In TI4, Carrier I and Carrier II roll 1 die hitting on 9+
      return Math.round(calculateHitChance(9, shift) * 100) / 100;
    }
    default:
      return 0;
  }
}

/**
 * Expected combat hits per round for ground combat units.
 * Accounts for public information: unit upgrades and faction abilities (Sardakk, Jol-Nar, Sol Spec Ops, Naaz-Rokha Mech).
 */
export function getExpectedGroundHits(
  unitType: string,
  context?: CombatPublicContext | PlayerView | null,
): number {
  const base = getUnitBaseType(unitType);
  const faction = context?.faction;
  const technologies = context?.technologies;
  const shift = getFactionCombatModifier(faction);
  const upgraded = isUnitUpgraded(unitType, base, technologies);
  const normType = unitType.toLowerCase().replace(/[-_\s]/g, "");

  switch (base) {
    case "mech": {
      // Naaz-Rokha Mech (Z-Grav Eidolon) rolls 2 dice hitting on 6+
      const isNaaz =
        normType.includes("naaz") || normType.includes("eidolon") || faction === "naaz";
      const dice = isNaaz ? 2 : 1;
      return Math.round(dice * calculateHitChance(6, shift) * 100) / 100;
    }
    case "infantry": {
      // Sol Spec Ops I hits on 7+, Spec Ops II hits on 6+
      if (faction === "sol" || normType.includes("specops")) {
        const target = upgraded ? 6 : 7;
        return Math.round(calculateHitChance(target, shift) * 100) / 100;
      }
      // Standard Infantry: 1 die hitting on 8+ (Infantry I) or 7+ (Infantry II)
      const target = upgraded ? 7 : 8;
      return Math.round(calculateHitChance(target, shift) * 100) / 100;
    }
    case "pds": {
      // Titans Hel-Titan can act as a ground force (I hits on 7+, II hits on 6+)
      if (normType.includes("titans") || normType.includes("heltitan") || faction === "titans") {
        const target = upgraded ? 6 : 7;
        return Math.round(calculateHitChance(target, shift) * 100) / 100;
      }
      return 0;
    }
    default:
      return 0;
  }
}

/**
 * Checks if a unit type has the Sustain Damage ability by default or via public upgrades.
 */
export function hasSustainDamage(
  unitType: string,
  context?: CombatPublicContext | PlayerView | null,
): boolean {
  const base = getUnitBaseType(unitType);
  if (base === "warsun" || base === "flagship" || base === "dreadnought" || base === "mech") {
    return true;
  }
  const normType = unitType.toLowerCase().replace(/[-_\s]/g, "");
  const technologies = context?.technologies;
  const upgraded = isUnitUpgraded(unitType, base, technologies);

  // Saturn Engine II (Titans Cruiser II) has sustain damage
  if (
    base === "cruiser" &&
    upgraded &&
    (normType.includes("saturn") || normType.includes("titans") || context?.faction === "titans")
  ) {
    return true;
  }
  // Sol Advanced Carrier II has sustain damage
  if (
    base === "carrier" &&
    upgraded &&
    (normType.includes("sol") || normType.includes("advanced") || context?.faction === "sol")
  ) {
    return true;
  }
  // Titans Hel-Titan (PDS) has sustain damage
  if (
    base === "pds" &&
    (normType.includes("titans") || normType.includes("heltitan") || context?.faction === "titans")
  ) {
    return true;
  }
  return false;
}

function buildPlayerLookup(
  players?: readonly PlayerView[] | Record<string, PlayerView> | null,
): Map<string, PlayerView> {
  const map = new Map<string, PlayerView>();
  if (!players) return map;
  if (Array.isArray(players)) {
    for (const p of players) {
      if (p && p.id) map.set(p.id, p);
    }
  } else if (typeof players === "object") {
    for (const [id, p] of Object.entries(players)) {
      if (p) map.set(id, p);
    }
  }
  return map;
}

/**
 * Computes the aggregated economic metrics for a tile.
 */
export function computeTileEconomy(tile: TilePresentation): OverlayTileEconomy {
  let totalResources = 0;
  let totalInfluence = 0;
  let readyResources = 0;
  let readyInfluence = 0;
  let exhaustedResources = 0;
  let exhaustedInfluence = 0;

  const planets = tile.planets.map((p) => {
    const res = p.resources || 0;
    const inf = p.influence || 0;
    totalResources += res;
    totalInfluence += inf;

    if (p.exhausted) {
      exhaustedResources += res;
      exhaustedInfluence += inf;
    } else {
      readyResources += res;
      readyInfluence += inf;
    }

    return {
      id: p.id,
      label: p.label,
      resources: res,
      influence: inf,
      exhausted: p.exhausted,
      controlledBy: p.controlledBy,
      controllerColor: p.controllerColor,
    };
  });

  return {
    totalResources,
    totalInfluence,
    readyResources,
    readyInfluence,
    exhaustedResources,
    exhaustedInfluence,
    hasPlanets: planets.length > 0,
    planets,
  };
}

/**
 * Checks if a unit type can participate in space combat as a ship or space combatant.
 * Ground units (infantry, standard mechs) and structures (pds, spacedock) cannot participate.
 */
export function canParticipateInSpaceCombat(
  unitType: string,
  context?: CombatPublicContext | PlayerView | null,
): boolean {
  const base = getUnitBaseType(unitType);
  if (
    base === "warsun" ||
    base === "flagship" ||
    base === "dreadnought" ||
    base === "cruiser" ||
    base === "carrier" ||
    base === "destroyer" ||
    base === "fighter"
  ) {
    return true;
  }
  const normType = unitType.toLowerCase().replace(/[-_\s]/g, "");
  // Naaz-Rokha Z-Grav Eidolon in space can participate in space combat as if it were a ship
  if (
    (normType.includes("naaz") && normType.includes("space")) ||
    (normType.includes("eidolon") && normType.includes("space")) ||
    (base === "mech" &&
      (normType.includes("space") || (context?.faction === "naaz" && normType.includes("eidolon"))))
  ) {
    return true;
  }
  return false;
}

/**
 * Computes space combat units and expected hits per round for each player in space.
 * Only ships and units capable of participating in space combat are counted (e.g. transported infantry are excluded).
 */
export function computeTileSpaceCombat(
  tile: TilePresentation,
  players?: readonly PlayerView[] | Record<string, PlayerView> | null,
): OverlayTileSpaceCombat {
  const playerLookup = buildPlayerLookup(players);

  // Only units in space (planet == null or undefined) that can participate in space combat
  const spaceUnits = tile.units.filter(
    (u) => !u.planet && canParticipateInSpaceCombat(u.unitType, playerLookup.get(u.owner)),
  );
  if (spaceUnits.length === 0) {
    return { hasCombatUnits: false, fleets: [] };
  }

  // Group by owner
  const ownerMap = new Map<string, PlacedUnitPresentation[]>();
  for (const u of spaceUnits) {
    const list = ownerMap.get(u.owner) || [];
    list.push(u);
    ownerMap.set(u.owner, list);
  }

  const fleets: PlayerFleetCombatSummary[] = [];

  for (const [owner, units] of ownerMap.entries()) {
    const player = playerLookup.get(owner);
    let avgHits = 0;
    let sustainCount = 0;
    let capitalShipCount = 0;
    let fighterCount = 0;
    const unitCounts: Record<string, number> = {};

    for (const u of units) {
      const base = getUnitBaseType(u.unitType);
      unitCounts[base] = (unitCounts[base] || 0) + 1;

      if (base === "fighter") {
        fighterCount += 1;
      } else {
        capitalShipCount += 1;
      }

      avgHits += getExpectedSpaceHits(u.unitType, player);

      if (hasSustainDamage(u.unitType, player) && !u.damaged) {
        sustainCount += 1;
      }
    }

    const ownerColor = units[0]?.ownerColor || "#cbd5e1";
    fleets.push({
      owner,
      ownerColor,
      totalUnits: units.length,
      capitalShipCount,
      fighterCount,
      avgHits: Math.round(avgHits * 100) / 100,
      sustainCount,
      unitCounts,
    });
  }

  return {
    hasCombatUnits: fleets.length > 0,
    fleets,
  };
}

/**
 * Computes ground combat strength, defenders, and hits per planet.
 */
export function computeTileGroundCombat(
  tile: TilePresentation,
  players?: readonly PlayerView[] | Record<string, PlayerView> | null,
): OverlayTileGroundCombat {
  if (tile.planets.length === 0) {
    return { hasGroundForces: false, planets: [] };
  }

  const playerLookup = buildPlayerLookup(players);
  let hasGroundForces = false;
  const planetSummaries: PlanetGroundCombatSummary[] = tile.planets.map((planet) => {
    // Find units on this planet
    const planetUnits = tile.units.filter((u) => u.planet === planet.id);

    // Group by owner
    const ownerMap = new Map<string, PlacedUnitPresentation[]>();
    for (const u of planetUnits) {
      const list = ownerMap.get(u.owner) || [];
      list.push(u);
      ownerMap.set(u.owner, list);
    }

    let planetTotalDefenders = 0;
    const forces: PlanetGroundForcesSummary[] = [];

    for (const [owner, units] of ownerMap.entries()) {
      const player = playerLookup.get(owner);
      let infantryCount = 0;
      let mechCount = 0;
      let pdsCount = 0;
      let avgHits = 0;
      let sustainCount = 0;

      for (const u of units) {
        const base = getUnitBaseType(u.unitType);
        if (base === "infantry") {
          infantryCount += 1;
          avgHits += getExpectedGroundHits(u.unitType, player);
        } else if (base === "mech") {
          mechCount += 1;
          avgHits += getExpectedGroundHits(u.unitType, player);
          if (!u.damaged) sustainCount += 1;
        } else if (base === "pds") {
          pdsCount += 1;
        }
      }

      const totalUnits = infantryCount + mechCount;
      if (totalUnits > 0 || pdsCount > 0) {
        hasGroundForces = true;
        planetTotalDefenders += totalUnits;
      }

      const pdsShift = getFactionCombatModifier(player?.faction);
      const pdsUpgraded = isUnitUpgraded("pds", "pds", player?.technologies);
      const pdsTarget = pdsUpgraded ? 5 : 6;
      const pdsHitChance = calculateHitChance(pdsTarget, pdsShift);

      forces.push({
        owner,
        ownerColor: units[0]?.ownerColor || planet.controllerColor,
        infantryCount,
        mechCount,
        totalUnits,
        avgHits: Math.round(avgHits * 100) / 100,
        sustainCount,
        pdsCount,
        hasPlanetaryShield: pdsCount > 0,
        spaceCannonDefenseHits: Math.round(pdsCount * pdsHitChance * 100) / 100,
      });
    }

    return {
      id: planet.id,
      label: planet.label,
      controlledBy: planet.controlledBy,
      controllerColor: planet.controllerColor,
      forces,
      totalDefenders: planetTotalDefenders,
      hasForces: forces.length > 0,
    };
  });

  return {
    hasGroundForces,
    planets: planetSummaries,
  };
}

export const TECH_SPECIALTY_COLORS: Record<
  TechSpecialty,
  { bg: string; border: string; label: string; symbol: string }
> = {
  biotic: { bg: "#166534", border: "#22c55e", label: "Biotic", symbol: "🟢" },
  propulsion: { bg: "#0369a1", border: "#38bdf8", label: "Propulsion", symbol: "🔵" },
  cybernetic: { bg: "#854d0e", border: "#eab308", label: "Cybernetic", symbol: "🟡" },
  warfare: { bg: "#991b1b", border: "#ef4444", label: "Warfare", symbol: "🔴" },
};

/**
 * Normalizes any tech specialty string (case-insensitive, e.g. "BIOTIC" -> "biotic").
 */
export function normalizeTechSpecialty(spec: string | null | undefined): TechSpecialty | null {
  if (!spec) return null;
  const norm = String(spec).toLowerCase().trim();
  if (norm.includes("biotic") || norm.includes("green")) return "biotic";
  if (norm.includes("propulsion") || norm.includes("blue")) return "propulsion";
  if (norm.includes("cybernetic") || norm.includes("yellow")) return "cybernetic";
  if (norm.includes("warfare") || norm.includes("red")) return "warfare";
  return null;
}

/**
 * Safely resolves the visual styling and labels for a tech specialty.
 * Never returns undefined.
 */
export function getTechSpecialtyStyle(spec: string): {
  bg: string;
  border: string;
  label: string;
  symbol: string;
} {
  const norm = normalizeTechSpecialty(spec);
  if (norm && TECH_SPECIALTY_COLORS[norm]) {
    return TECH_SPECIALTY_COLORS[norm];
  }
  return {
    bg: "#1e293b",
    border: "#94a3b8",
    label: spec || "Tech",
    symbol: "🔬",
  };
}

/**
 * Computes tech specialty benefits and skips for a tile.
 */
export function computeTileTechBenefits(tile: TilePresentation): OverlayTileTechBenefits {
  const specialtyCounts: Record<TechSpecialty, number> = {
    biotic: 0,
    propulsion: 0,
    cybernetic: 0,
    warfare: 0,
  };

  const techPlanets: TechSpecialtyPlanetSummary[] = [];

  for (const p of tile.planets) {
    if (p.techSpecialties && p.techSpecialties.length > 0) {
      const normalizedSpecs: TechSpecialty[] = [];
      for (const spec of p.techSpecialties) {
        const norm = normalizeTechSpecialty(spec);
        if (norm) {
          normalizedSpecs.push(norm);
          if (specialtyCounts[norm] !== undefined) {
            specialtyCounts[norm] += 1;
          }
        }
      }
      if (normalizedSpecs.length > 0) {
        techPlanets.push({
          id: p.id,
          label: p.label,
          specialties: normalizedSpecs,
          traits: p.traits || [],
          exhausted: p.exhausted,
          controlledBy: p.controlledBy,
          controllerColor: p.controllerColor,
        });
      }
    }
  }

  return {
    hasTechSpecialties: techPlanets.length > 0,
    planets: techPlanets,
    specialtyCounts,
  };
}
