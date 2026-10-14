import type { PlacedUnitPresentation, SelectedSystemDetails } from "./boardPresentation.ts";
import { groupStructures, splitStructures, type StructureGroup } from "./planetStructures.ts";
import { getUnitBaseType, UNIT_PRIORITY, type UnitBaseType } from "../components/UnitIcon.tsx";

export interface GroupedUnit {
  baseType: UnitBaseType;
  totalCount: number;
  damagedCount: number;
}

/** One owner's units of a kind, counted per base type in the usual unit order. */
export function groupUnits(units: readonly PlacedUnitPresentation[]): GroupedUnit[] {
  const byType = new Map<UnitBaseType, { total: number; damaged: number }>();
  for (const u of units) {
    const baseType = getUnitBaseType(u.unitType);
    const counts = byType.get(baseType) ?? { total: 0, damaged: 0 };
    counts.total += 1;
    if (u.damaged) counts.damaged += 1;
    byType.set(baseType, counts);
  }
  return Array.from(byType.entries())
    .map(([baseType, c]) => ({ baseType, totalCount: c.total, damagedCount: c.damaged }))
    .sort((a, b) => (UNIT_PRIORITY[a.baseType] ?? 99) - (UNIT_PRIORITY[b.baseType] ?? 99));
}

export interface OwnerPlanetUnits {
  planetId: string;
  ground: GroupedUnit[];
  structures: StructureGroup[];
}

/** Everything one player has in a system: ships in space, then per planet ground forces and structures. */
export interface OwnerBlock {
  owner: string;
  ownerColor: string;
  space: GroupedUnit[];
  planets: OwnerPlanetUnits[];
}

/**
 * Units of a system strictly per owner: the viewer's block first, then the others in table order
 * (seats the table does not list come last, in first-seen order). A unit is only ever counted in
 * its own owner's block.
 */
export function buildOwnerBlocks(
  system: Pick<SelectedSystemDetails, "spaceUnits" | "planetUnits">,
  viewerSeat: string | null,
  seatingOrder: readonly string[],
): OwnerBlock[] {
  const colors = new Map<string, string>();
  const owners: string[] = [];
  const note = (u: PlacedUnitPresentation) => {
    if (!colors.has(u.owner)) {
      colors.set(u.owner, u.ownerColor);
      owners.push(u.owner);
    }
  };
  system.spaceUnits.forEach(note);
  Object.values(system.planetUnits).forEach((list) => list.forEach(note));
  const rank = (owner: string) => {
    if (owner === viewerSeat) return -1;
    const i = seatingOrder.indexOf(owner);
    return i < 0 ? Number.MAX_SAFE_INTEGER : i;
  };
  owners.sort((a, b) => rank(a) - rank(b));
  return owners.map((owner) => {
    const mine = (u: PlacedUnitPresentation) => u.owner === owner;
    const planets: OwnerPlanetUnits[] = [];
    for (const [planetId, all] of Object.entries(system.planetUnits)) {
      const { structures, others } = splitStructures(all.filter(mine));
      const ground = groupUnits(others);
      const groups = groupStructures(structures);
      if (ground.length > 0 || groups.length > 0) planets.push({ planetId, ground, structures: groups });
    }
    return {
      owner,
      ownerColor: colors.get(owner) ?? "",
      space: groupUnits(system.spaceUnits.filter(mine)),
      planets,
    };
  });
}
