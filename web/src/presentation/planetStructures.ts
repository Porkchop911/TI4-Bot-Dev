import type { PlacedUnitPresentation } from "./boardPresentation.ts";
import { describeUnit, resolveUnit } from "./factionInfo.ts";
import { getUnitBaseType } from "../components/UnitIcon.tsx";
import type { StructureKind } from "../components/StructureIcon.tsx";

/** A PDS or space dock standing on a planet (public board state). */
export function structureKind(unitType: string): StructureKind | null {
  const base = getUnitBaseType(unitType);
  return base === "pds" || base === "spacedock" ? base : null;
}

/** Structures on a planet leave the ordinary unit list, so nothing is listed twice. */
export function splitStructures(units: readonly PlacedUnitPresentation[]): {
  structures: PlacedUnitPresentation[];
  others: PlacedUnitPresentation[];
} {
  const structures: PlacedUnitPresentation[] = [];
  const others: PlacedUnitPresentation[] = [];
  for (const u of units) (u.planet && structureKind(u.unitType) ? structures : others).push(u);
  return { structures, others };
}

export interface StructureGroup {
  kind: StructureKind;
  owner: string;
  ownerColor: string;
  unitType: string;
  count: number;
  damaged: number;
}

/** One entry per owner and structure card; PDS before docks. */
export function groupStructures(structures: readonly PlacedUnitPresentation[]): StructureGroup[] {
  const groups = new Map<string, StructureGroup>();
  for (const u of structures) {
    const kind = structureKind(u.unitType);
    if (!kind) continue;
    const key = `${u.owner}|${u.unitType}`;
    const g = groups.get(key) ?? {
      kind,
      owner: u.owner,
      ownerColor: u.ownerColor,
      unitType: u.unitType,
      count: 0,
      damaged: 0,
    };
    g.count += 1;
    if (u.damaged) g.damaged += 1;
    groups.set(key, g);
  }
  return [...groups.values()].sort((a, b) => (a.kind === b.kind ? 0 : a.kind === "pds" ? -1 : 1));
}

export interface StructureCard {
  name: string;
  upgraded: boolean;
  /** "Production 2", "Space cannon 6 (x3)": only what the catalog carries. */
  details: string[];
}

/** The seat's card for this structure (II upgrade and faction variant applied). */
export function structureCard(
  unitType: string,
  faction: string | null,
  technologies: readonly string[],
): StructureCard | undefined {
  const resolved = resolveUnit(unitType, faction, technologies);
  if (!resolved) return undefined;
  const d = describeUnit(resolved);
  const details = [
    ...d.facts.filter((f) => f.label === "Production").map((f) => `Production ${f.value}`),
    ...d.keywords.filter((k) => /^Space cannon/.test(k)),
  ];
  return { name: d.name, upgraded: d.upgraded, details };
}
