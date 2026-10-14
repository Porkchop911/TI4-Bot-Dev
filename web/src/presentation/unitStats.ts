import type { ResolvedUnit, UnitRecord } from "./factionInfo.ts";

/**
 * The at-a-glance numbers of a build option: cost, combat with its hit chance, movement, capacity
 * and the ability keywords. All of it is read from the resolved unit card (catalog values for the
 * seat's faction and upgrades); a number the catalog lacks gives no entry, never an invented one.
 */

export interface DiceStat {
  /** The die roll that hits: 8 means "8 or higher". */
  hitsOn: number;
  dice: number;
  /** Chance that one die hits, 0-100 (TI4 d10: a roll of N or higher hits, so (11 - N) / 10). */
  chancePercent: number;
  /** Expected hits per roll of all dice, e.g. 0.6 for two dice that hit on 8. */
  expectedHits: number;
}

export interface UnitStats {
  /** The unit's printed cost per unit; below 1 means several units per resource. */
  cost?: number;
  /** Combat, or `undefined` for a unit that does not fight (a PDS, a space dock). */
  combat?: DiceStat;
  move?: number;
  capacity?: number;
  sustain: boolean;
  bombardment?: DiceStat;
  antiFighterBarrage?: DiceStat;
  spaceCannon?: DiceStat & { deep: boolean };
  /** A planetary shield, or a unit that switches the enemy's off. */
  planetaryShield: boolean;
  disablesPlanetaryShield: boolean;
  production?: string;
  /** The card has printed text beyond the numbers (a flagship or mech ability, a faction variant). */
  hasSpecialText: boolean;
  /** The card is the "II" card or the faction's own version. */
  upgraded: boolean;
  factionSpecific: boolean;
}

/** One die that hits on `hitsOn` or higher on a d10. A value outside 1-10 is clamped into 0-100%. */
export function hitChancePercent(hitsOn: number): number {
  return Math.min(100, Math.max(0, (11 - hitsOn) * 10));
}

export function diceStat(hitsOn: number | undefined, count: number | undefined): DiceStat | undefined {
  if (hitsOn === undefined || count === 0) return undefined;
  const dice = count && count > 0 ? count : 1;
  const chancePercent = hitChancePercent(hitsOn);
  return {
    hitsOn,
    dice,
    chancePercent,
    expectedHits: Math.round(dice * chancePercent) / 100,
  };
}

/** "8 ×2 · 30% per die" (one die: "9 · 20%"). */
export function formatDice(stat: DiceStat): string {
  return stat.dice > 1
    ? `${stat.hitsOn} ×${stat.dice} · ${stat.chancePercent}% per die`
    : `${stat.hitsOn} · ${stat.chancePercent}%`;
}

/** "3", or "2 for 1" for a unit bought two to a resource. */
export function formatUnitCost(cost: number): string {
  if (cost > 0 && cost < 1) return `${Math.round(1 / cost)} for 1`;
  return String(cost);
}

const unitStatsOf = (u: UnitRecord, resolved: Pick<ResolvedUnit, "upgraded" | "factionSpecific">): UnitStats => {
  const spaceCannon = diceStat(u.spaceCannonHitsOn, u.spaceCannonDieCount);
  return {
    cost: u.cost,
    combat: diceStat(u.combatHitsOn, u.combatDieCount),
    move: u.moveValue,
    capacity: u.capacityValue,
    sustain: !!u.sustainDamage,
    bombardment: diceStat(u.bombardHitsOn, u.bombardDieCount),
    antiFighterBarrage: diceStat(u.afbHitsOn, u.afbDieCount),
    spaceCannon: spaceCannon && { ...spaceCannon, deep: !!u.deepSpaceCannon },
    planetaryShield: !!u.planetaryShield,
    disablesPlanetaryShield: !!u.disablesPlanetaryShield,
    production: u.productionValue === undefined ? undefined : String(u.productionValue),
    hasSpecialText: !!u.ability && (resolved.factionSpecific || u.baseType === "flagship" || u.baseType === "mech"),
    upgraded: resolved.upgraded,
    factionSpecific: resolved.factionSpecific,
  };
};

export function describeUnitStats(resolved: ResolvedUnit): UnitStats {
  return unitStatsOf(resolved.unit, resolved);
}

export interface Chip {
  /** Short visible text. */
  text: string;
  /** What it means in full, for the tooltip and the accessible name. */
  title: string;
}

/** The ability chips of a unit: sustain, bombardment, anti-fighter barrage, space cannon, production... */
export function keywordChips(stats: UnitStats): Chip[] {
  const chips: Chip[] = [];
  if (stats.sustain) chips.push({ text: "Sustain", title: "Sustain damage" });
  if (stats.bombardment) {
    chips.push({ text: `Bombard ${formatDice(stats.bombardment)}`, title: "Bombardment" });
  }
  if (stats.antiFighterBarrage) {
    chips.push({ text: `AFB ${formatDice(stats.antiFighterBarrage)}`, title: "Anti-fighter barrage" });
  }
  if (stats.spaceCannon) {
    chips.push({
      text: `Space cannon ${formatDice(stats.spaceCannon)}`,
      title: stats.spaceCannon.deep ? "Deep space cannon" : "Space cannon",
    });
  }
  if (stats.planetaryShield) chips.push({ text: "Planetary shield", title: "Planetary shield" });
  if (stats.disablesPlanetaryShield) {
    chips.push({ text: "Ignores shield", title: "Disables planetary shield" });
  }
  if (stats.production) chips.push({ text: `Production ${stats.production}`, title: "Production" });
  return chips;
}
