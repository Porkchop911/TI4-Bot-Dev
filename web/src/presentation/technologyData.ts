import type { BoardView, PlayerView } from "../protocol/types.ts";
import {
  getTechnologyMeta,
  findPlanetMeta,
  TECHNOLOGIES,
  humanizeId,
  type TechnologyMeta,
} from "../protocol/contentCatalog.ts";

export interface TechTrackDefinition {
  id: "PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE";
  name: string;
  colorName: string;
  accentColor: string;
  badgeBg: string;
  badgeBorder: string;
  techIds: readonly string[];
}

export const TECH_TRACKS: readonly TechTrackDefinition[] = [
  {
    id: "PROPULSION",
    name: "Propulsion",
    colorName: "Blue",
    accentColor: "#38bdf8",
    badgeBg: "rgba(56, 189, 248, 0.15)",
    badgeBorder: "rgba(56, 189, 248, 0.4)",
    techIds: ["amd", "det", "gd", "sr", "fl", "lwd"],
  },
  {
    id: "BIOTIC",
    name: "Biotic",
    colorName: "Green",
    accentColor: "#4ade80",
    badgeBg: "rgba(74, 222, 128, 0.15)",
    badgeBorder: "rgba(74, 222, 128, 0.4)",
    techIds: ["nm", "pa", "dxa", "bs", "hm", "x89c4"],
  },
  {
    id: "CYBERNETIC",
    name: "Cybernetic",
    colorName: "Yellow",
    accentColor: "#facc15",
    badgeBg: "rgba(250, 204, 21, 0.15)",
    badgeBorder: "rgba(250, 204, 21, 0.4)",
    techIds: ["st", "sdn", "gls", "pi", "td", "ie"],
  },
  {
    id: "WARFARE",
    name: "Warfare",
    colorName: "Red",
    accentColor: "#f87171",
    badgeBg: "rgba(248, 113, 113, 0.15)",
    badgeBorder: "rgba(248, 113, 113, 0.4)",
    techIds: ["ps", "aida", "md", "sar", "da", "asc"],
  },
] as const;

export const UNIT_UPGRADE_TECH_IDS: readonly string[] = [
  "cr2",
  "dn2",
  "cv2",
  "dd2",
  "ff2",
  "ws",
  "inf2",
  "sd2",
  "pds2",
] as const;

export interface TechTierGroup {
  tier: number;
  label: string;
  rows: readonly (readonly [string, string, string, string])[];
}

export const TECH_TIERS: readonly TechTierGroup[] = [
  {
    tier: 0,
    label: "0 Prerequisites",
    rows: [
      ["amd", "nm", "st", "ps"],
      ["det", "pa", "sdn", "aida"],
    ],
  },
  {
    tier: 1,
    label: "1 Prerequisite",
    rows: [
      ["gd", "dxa", "gls", "md"],
      ["sr", "bs", "pi", "sar"],
    ],
  },
  {
    tier: 2,
    label: "2 Prerequisites",
    rows: [["fl", "hm", "td", "da"]],
  },
  {
    tier: 3,
    label: "3 Prerequisites",
    rows: [["lwd", "x89c4", "ie", "asc"]],
  },
] as const;

/** Alias equivalence sets (e.g. replacement/omega printings) */
const TECH_ALIAS_MAP: Record<string, string[]> = {
  x89c4: ["x89c4", "x89", "x89_base"],
  x89: ["x89c4", "x89", "x89_base"],
  md: ["md", "md_base", "md_c1"],
};

export function techMatches(actual: string, target: string): boolean {
  if (actual === target) return true;
  const aliases = TECH_ALIAS_MAP[target];
  if (aliases && aliases.includes(actual)) return true;
  return false;
}

export function hasResearchedTech(player: PlayerView, techId: string): boolean {
  if (!player.technologies || player.technologies.length === 0) return false;
  return player.technologies.some((id) => techMatches(id, techId));
}

export function isTechExhausted(player: PlayerView, techId: string): boolean {
  if (!player.exhausted_technologies || player.exhausted_technologies.length === 0) return false;
  return player.exhausted_technologies.some((id) => techMatches(id, techId));
}

export interface PrereqSymbol {
  type: "B" | "G" | "Y" | "R";
  label: string;
  color: string;
  bgColor: string;
}

export function parseRequirements(requirements?: string): PrereqSymbol[] {
  if (!requirements || requirements === "-") return [];
  const result: PrereqSymbol[] = [];
  for (const char of requirements) {
    switch (char) {
      case "B":
        result.push({
          type: "B",
          label: "B",
          color: "#38bdf8",
          bgColor: "rgba(56, 189, 248, 0.25)",
        });
        break;
      case "G":
        result.push({
          type: "G",
          label: "G",
          color: "#4ade80",
          bgColor: "rgba(74, 222, 128, 0.25)",
        });
        break;
      case "Y":
        result.push({
          type: "Y",
          label: "Y",
          color: "#facc15",
          bgColor: "rgba(250, 204, 21, 0.25)",
        });
        break;
      case "R":
        result.push({
          type: "R",
          label: "R",
          color: "#f87171",
          bgColor: "rgba(248, 113, 113, 0.25)",
        });
        break;
      default:
        break;
    }
  }
  return result;
}

export interface HydratedTech {
  id: string;
  meta: TechnologyMeta;
  prereqs: PrereqSymbol[];
  prereqCount: number;
}

export function hydrateTech(id: string): HydratedTech {
  const meta = getTechnologyMeta(id);
  const prereqs = parseRequirements(meta.requirements);
  return {
    id,
    meta,
    prereqs,
    prereqCount: prereqs.length,
  };
}

export function getFactionTechIds(faction?: string): string[] {
  if (!faction) return [];
  const normalized = faction.toLowerCase();
  return Object.values(TECHNOLOGIES)
    .filter(
      (meta) =>
        "faction" in meta &&
        typeof (meta as { faction?: unknown }).faction === "string" &&
        (meta as { faction: string }).faction.toLowerCase() === normalized,
    )
    .map((meta) => meta.id);
}

export function getTechnologyTrack(
  techId: string,
): "PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE" | null {
  const meta = getTechnologyMeta(techId);
  if (meta?.types) {
    for (const t of meta.types) {
      const upper = t.toUpperCase();
      if (
        upper === "PROPULSION" ||
        upper === "BIOTIC" ||
        upper === "CYBERNETIC" ||
        upper === "WARFARE"
      ) {
        return upper;
      }
    }
  }
  for (const track of TECH_TRACKS) {
    if (track.techIds.some((id) => techMatches(id, techId))) {
      return track.id;
    }
  }
  return null;
}

export function isTechReplacedForFaction(techId: string, faction?: string): boolean {
  if (!faction) return false;
  const normalized = faction.toLowerCase();
  return Object.values(TECHNOLOGIES).some(
    (meta) =>
      "faction" in meta &&
      (meta as { faction?: string }).faction?.toLowerCase() === normalized &&
      "baseUpgrade" in meta &&
      (meta as { baseUpgrade?: string }).baseUpgrade === techId,
  );
}

export function isTechAllowedForFaction(tech: HydratedTech, faction?: string): boolean {
  const normFaction = faction?.toLowerCase();
  if (tech.meta.faction && tech.meta.faction.toLowerCase() !== normFaction) {
    return false;
  }
  if (isTechReplacedForFaction(tech.id, faction)) {
    return false;
  }
  return true;
}

export interface ControlledSpecialtyPlanet {
  planetId: string;
  name: string;
  specialty: "PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE";
  isExhausted: boolean;
}

export function getControlledSpecialtyPlanets(
  board: BoardView | null | undefined,
  playerId?: string | null,
): ControlledSpecialtyPlanet[] {
  if (!board || !playerId) return [];
  const results: ControlledSpecialtyPlanet[] = [];
  const seenPlanets = new Set<string>();

  for (const system of Object.values(board.systems || {})) {
    for (const planet of Object.values(system.planets || {})) {
      if (planet.controlled_by === playerId && !seenPlanets.has(planet.planet_id)) {
        seenPlanets.add(planet.planet_id);
        const meta = findPlanetMeta(planet.planet_id);
        if (meta && meta.techSpecialties && meta.techSpecialties.length > 0) {
          for (const spec of meta.techSpecialties) {
            const upper = spec.toUpperCase();
            if (
              upper === "PROPULSION" ||
              upper === "BIOTIC" ||
              upper === "CYBERNETIC" ||
              upper === "WARFARE"
            ) {
              results.push({
                planetId: planet.planet_id,
                name: meta.name || humanizeId(planet.planet_id),
                specialty: upper,
                isExhausted: Boolean(planet.exhausted),
              });
            }
          }
        }
      }
    }
  }
  return results;
}

export function checkTechPrerequisites(
  tech: HydratedTech,
  ownedTrackCounts: Record<"PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE", number>,
  toggledSkips: Record<"PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE", number>,
  faction?: string,
): boolean {
  if (tech.prereqs.length === 0) return true;

  const needed: Record<string, number> = { B: 0, G: 0, Y: 0, R: 0 };
  for (const p of tech.prereqs) {
    needed[p.type] = (needed[p.type] || 0) + 1;
  }

  const colorMap: Record<string, "PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE"> = {
    B: "PROPULSION",
    G: "BIOTIC",
    Y: "CYBERNETIC",
    R: "WARFARE",
  };

  let waiverBudget = 0;
  if (faction?.toLowerCase() === "jolnar") {
    waiverBudget = tech.meta.types?.includes("unit_upgrade") ? 1 : 2;
  }

  let totalMissing = 0;
  for (const [letter, reqCount] of Object.entries(needed)) {
    if (reqCount === 0) continue;
    const track = colorMap[letter];
    const available = (ownedTrackCounts[track] || 0) + (toggledSkips[track] || 0);
    if (available < reqCount) {
      totalMissing += reqCount - available;
    }
  }

  return totalMissing <= waiverBudget;
}
