import { BoardView, PlayerView } from "../protocol/types.ts";
import { getUnitBaseType } from "../components/UnitIcon.tsx";
import {
  findPlanetMeta,
  findAttachmentMeta,
  PLANETS,
  ATTACHMENTS,
} from "../protocol/contentCatalog.ts";

export { PLANETS as STATIC_PLANETS, ATTACHMENTS as ATTACHMENT_MODIFIERS };

export interface PlayerComputedStats {
  remainingResources: number;
  totalResources: number;
  remainingInfluence: number;
  totalInfluence: number;
  controlledSystems: number;
  controlledPlanets: number;
  availableProductionCapacity: number;
  totalProductionCapacity: number;
  /** Backwards compatibility alias for totalProductionCapacity */
  productionCapacity: number;
}

/** Returns true if unit is a ship (consumes fleet supply or is a fighter). */
export function isShip(unitType: string): boolean {
  const base = getUnitBaseType(unitType);
  return (
    base === "flagship" ||
    base === "warsun" ||
    base === "dreadnought" ||
    base === "carrier" ||
    base === "cruiser" ||
    base === "destroyer" ||
    base === "fighter"
  );
}

export function getPlanetEffectiveValues(
  planetId: string,
  attachments?: string[],
  mapTiles?: BoardView["map_tiles"],
): { resources: number; influence: number } {
  let baseResources = 0;
  let baseInfluence = 0;
  let found = false;

  // 1. Look up in map_tiles first
  if (mapTiles) {
    for (const tile of mapTiles) {
      if (tile.planets) {
        for (const p of tile.planets) {
          if (p.id === planetId) {
            baseResources = p.resources;
            baseInfluence = p.influence;
            found = true;
            break;
          }
        }
      }
      if (found) break;
    }
  }

  // 2. Fall back to generated static catalog
  if (!found) {
    const meta = findPlanetMeta(planetId);
    if (meta) {
      baseResources = meta.resources;
      baseInfluence = meta.influence;
    } else {
      const norm = planetId.toLowerCase().replace(/[-_\s]/g, "");
      const entry = (PLANETS as Record<string, { resources: number; influence: number }>)[norm];
      if (entry) {
        baseResources = entry.resources;
        baseInfluence = entry.influence;
      } else if (norm.includes("mecatol")) {
        baseResources = 1;
        baseInfluence = 6;
      } else if (norm.includes("jord")) {
        baseResources = 4;
        baseInfluence = 2;
      }
    }
  }

  // 3. Add attachment modifiers from generated catalog
  let res = baseResources;
  let inf = baseInfluence;
  if (attachments && attachments.length > 0) {
    for (const att of attachments) {
      const normAtt = att.toLowerCase().replace(/[-_\s]/g, "");
      const mod =
        findAttachmentMeta(att) ??
        (ATTACHMENTS as Record<string, { resourcesModifier: number; influenceModifier: number }>)[
          normAtt
        ];
      if (mod) {
        res += mod.resourcesModifier ?? 0;
        inf += mod.influenceModifier ?? 0;
      }
    }
  }

  return { resources: res, influence: inf };
}

/**
 * Computes live player statistics based on the galaxy board state:
 * - Remaining / Total resources from controlled planets
 * - Remaining / Total influence from controlled planets
 * - Controlled systems (LRR 80)
 * - Controlled planets
 * - Total production capacity
 */
export function computePlayerStats(
  player: PlayerView,
  board?: BoardView | null,
): PlayerComputedStats {
  if (!board || !board.systems) {
    return {
      remainingResources: 0,
      totalResources: 0,
      remainingInfluence: 0,
      totalInfluence: 0,
      controlledSystems: 0,
      controlledPlanets: 0,
      availableProductionCapacity: 0,
      totalProductionCapacity: 0,
      productionCapacity: 0,
    };
  }

  let remainingResources = 0;
  let totalResources = 0;
  let remainingInfluence = 0;
  let totalInfluence = 0;
  let controlledSystems = 0;
  let controlledPlanets = 0;
  let availableProductionCapacity = 0;
  let totalProductionCapacity = 0;

  const mapTiles = board.map_tiles;
  const playerId = player.id;
  const factionNorm = player.faction?.toLowerCase().replace(/[-_\s]/g, "") || "";

  const hasSpaceDock2 =
    player.technologies?.some((t) => {
      const norm = t.toLowerCase().replace(/[-_\s]/g, "");
      return norm === "sd2" || norm === "spacedock2" || norm === "helios2";
    }) ?? false;

  const hasLetaniWarrior2 =
    player.technologies?.some((t) => {
      const norm = t.toLowerCase().replace(/[-_\s]/g, "");
      return norm === "lw2" || norm === "letaniwarrior2" || norm === "arborecinfantry2";
    }) ?? false;

  const hasFloatingFactory2 =
    player.technologies?.some((t) => {
      const norm = t.toLowerCase().replace(/[-_\s]/g, "");
      return norm === "floatingfactory2" || norm === "saarspacedock2";
    }) ?? false;

  const hasDimensionalTear2 =
    player.technologies?.some((t) => {
      const norm = t.toLowerCase().replace(/[-_\s]/g, "");
      return norm === "dt2" || norm === "dimensionaltear2" || norm === "cabalspacedock2";
    }) ?? false;

  const planetCache = new Map<string, { resources: number; influence: number }>();
  const getPlanetValues = (planetId: string, attachments?: string[]) => {
    const key = `${planetId}:${(attachments || []).join(",")}`;
    let cached = planetCache.get(key);
    if (!cached) {
      cached = getPlanetEffectiveValues(planetId, attachments, mapTiles);
      planetCache.set(key, cached);
    }
    return cached;
  };

  for (const system of Object.values(board.systems)) {
    const planets = Object.values(system.planets || {});
    const units = system.units || [];

    // 1. Controlled Planets & Resources / Influence
    for (const p of planets) {
      if (p.controlled_by === playerId) {
        controlledPlanets++;
        const vals = getPlanetValues(p.planet_id, p.attachments);
        totalResources += vals.resources;
        totalInfluence += vals.influence;
        if (!p.exhausted) {
          remainingResources += vals.resources;
          remainingInfluence += vals.influence;
        }
      }
    }

    // 2. Controlled Systems (LRR 80)
    // 80. A player controls a system if they control all of the planets in the system
    // and there are no other players' ships in the system.
    // 80.1a. If no planets, player controls it if they are the only player with ships.
    // 80.1b. If no planets and no ships, no player controls it.
    const hasEnemyShips = units.some((u) => u.owner !== playerId && isShip(u.unit_type));
    if (!hasEnemyShips) {
      if (planets.length > 0) {
        const controlsAllPlanets = planets.every((p) => p.controlled_by === playerId);
        if (controlsAllPlanets) {
          controlledSystems++;
        }
      } else {
        const hasOwnShips = units.some((u) => u.owner === playerId && isShip(u.unit_type));
        if (hasOwnShips) {
          controlledSystems++;
        }
      }
    }

    // 3. Production Capacity (activated systems count 0 for available capacity)
    const isActivatedByPlayer = system.command_tokens?.includes(playerId) ?? false;
    let systemProductionCapacity = 0;

    for (const u of units) {
      if (u.owner !== playerId) continue;
      const baseType = getUnitBaseType(u.unit_type);
      const unitNorm = u.unit_type.toLowerCase().replace(/[-_\s]/g, "");

      if (baseType === "spacedock") {
        if (
          factionNorm.includes("saar") ||
          unitNorm.includes("floatingfactory") ||
          unitNorm.includes("saar")
        ) {
          systemProductionCapacity += hasFloatingFactory2 || unitNorm.includes("2") ? 7 : 5;
        } else if (
          factionNorm.includes("cabal") ||
          unitNorm.includes("dimensionaltear") ||
          unitNorm.includes("cabal")
        ) {
          systemProductionCapacity += hasDimensionalTear2 || unitNorm.includes("2") ? 7 : 5;
        } else {
          // Standard Space Dock: 2 + planet printed resources (or 4 + planet resources if SD2)
          const bonus = hasSpaceDock2 || unitNorm.includes("2") ? 4 : 2;
          let planetRes = 0;
          if (u.planet) {
            const planetView = system.planets?.[u.planet];
            planetRes = getPlanetValues(u.planet, planetView?.attachments).resources;
          } else if (planets.length === 1 && planets[0].controlled_by === playerId) {
            planetRes = getPlanetValues(planets[0].planet_id, planets[0].attachments).resources;
          }
          systemProductionCapacity += planetRes + bonus;
        }
      } else if (
        baseType === "infantry" &&
        (factionNorm.includes("arborec") ||
          unitNorm.includes("letani") ||
          unitNorm.includes("arborec"))
      ) {
        // Arborec Letani Warrior: 1 (or 2 if upgraded)
        systemProductionCapacity += hasLetaniWarrior2 || unitNorm.includes("2") ? 2 : 1;
      } else if (
        baseType === "mech" &&
        (factionNorm.includes("arborec") ||
          unitNorm.includes("letani") ||
          unitNorm.includes("behemoth"))
      ) {
        // Arborec Mech: Letani Behemoth
        systemProductionCapacity += 2;
      } else if (
        baseType === "pds" &&
        (factionNorm.includes("titans") ||
          unitNorm.includes("heltitan") ||
          unitNorm.includes("titan"))
      ) {
        // Titans Hel-Titan
        systemProductionCapacity += 1;
      } else if (unitNorm.includes("bastion") && baseType === "flagship") {
        systemProductionCapacity += 1;
      } else if (unitNorm.includes("deepwrought") || unitNorm.includes("eanautic")) {
        systemProductionCapacity += 1;
      }
    }

    totalProductionCapacity += systemProductionCapacity;
    if (!isActivatedByPlayer) {
      availableProductionCapacity += systemProductionCapacity;
    }
  }

  return {
    remainingResources,
    totalResources,
    remainingInfluence,
    totalInfluence,
    controlledSystems,
    controlledPlanets,
    availableProductionCapacity,
    totalProductionCapacity,
    productionCapacity: totalProductionCapacity,
  };
}
