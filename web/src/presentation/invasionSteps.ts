import type { InvasionStepView, PlacedUnitView } from "../protocol/types.ts";
import { findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";
import { countBy, type UnitCount } from "./combatSummary.ts";

/** Structures sit on a planet but are never ground forces: bombardment and Space Cannon hits skip them. */
const STRUCTURES = new Set(["pds", "space_dock", "spacedock"]);

export type InvasionStepKind = "bombardment" | "space_cannon_defense";

/** The dice of one unit type (and one hit threshold) that fired in a step. */
export interface DiceGroup {
  seat: string;
  unit: string;
  /** The result needed to hit ("5" means 5 or higher). */
  target: number;
  faces: { face: number; hit: boolean }[];
  hits: number;
}

/** What a step did to one player's ground forces on the planet. */
export interface StepSideEffect {
  seat: string;
  before: UnitCount[];
  lost: UnitCount[];
  sustained: UnitCount[];
  remaining: number;
}

export interface InvasionStepSummary {
  kind: InvasionStepKind;
  planet: string;
  /** Who rolled: the invader for bombardment, the planet's defenders for Space Cannon Defense. */
  shooters: DiceGroup[];
  dice: number;
  hits: number;
  /** Whose ground forces the hits could fall on (those with ground forces on the planet). */
  targets: StepSideEffect[];
  /** Ground forces destroyed in total. */
  destroyed: number;
  /** Hits that cost nothing: a Mech sustained damage, or there was nothing left to hit. */
  absorbed: number;
}

export const STEP_TITLES: Record<InvasionStepKind, string> = {
  bombardment: "Bombardment",
  space_cannon_defense: "Space Cannon Defense",
};

export const isDefenseStep = (kind: string): kind is InvasionStepKind =>
  kind === "bombardment" || kind === "space_cannon_defense";

export function planetName(planet: string): string {
  return findPlanetMeta(planet)?.name ?? humanizeId(planet);
}

const groundForces = (units: readonly PlacedUnitView[], seat: string) =>
  units.filter((u) => u.owner === seat && !STRUCTURES.has(u.unit_type.toLowerCase()));

const toList = (counts: Map<string, number>): UnitCount[] =>
  [...counts].map(([unit, count]) => ({ unit, count }));

/** What happened to `seat`'s ground forces between two states of the planet. */
export function sideEffect(
  seat: string,
  before: readonly PlacedUnitView[],
  after: readonly PlacedUnitView[],
): StepSideEffect {
  const was = groundForces(before, seat);
  const now = groundForces(after, seat);
  const wasCounts = countBy(was);
  const nowCounts = countBy(now);
  const wasDamaged = countBy(was.filter((u) => u.damaged));
  const nowDamaged = countBy(now.filter((u) => u.damaged));
  const lost: UnitCount[] = [];
  const sustained: UnitCount[] = [];
  for (const [unit, count] of wasCounts) {
    const gone = count - (nowCounts.get(unit) ?? 0);
    if (gone > 0) lost.push({ unit, count: gone });
  }
  for (const [unit, count] of nowDamaged) {
    const fresh = Math.min(count - (wasDamaged.get(unit) ?? 0), nowCounts.get(unit) ?? 0);
    if (fresh > 0) sustained.push({ unit, count: fresh });
  }
  return { seat, before: toList(wasCounts), lost, sustained, remaining: now.length };
}

/**
 * A bombardment or Space Cannon Defense step as the server sends it (`last_step`), seen as who
 * rolled which dice, how many hit and what it cost the ground forces on the planet. `null` for
 * every other kind of step.
 */
export function summarizeInvasionStep(
  step: InvasionStepView | null | undefined,
  invader: string,
): InvasionStepSummary | null {
  if (!step || !isDefenseStep(step.kind)) return null;
  const groups = new Map<string, DiceGroup>();
  for (const die of step.dice) {
    const key = `${die.player}|${die.group}|${die.target}`;
    let group = groups.get(key);
    if (!group) {
      group = { seat: die.player, unit: die.group, target: die.target, faces: [], hits: 0 };
      groups.set(key, group);
    }
    group.faces.push({ face: die.face, hit: die.hit });
    if (die.hit) group.hits += 1;
  }
  const shooters = [...groups.values()];
  const dice = step.dice.length;
  // Dice are the truth when the step lists them; otherwise the server's hit totals.
  const hits = dice > 0 ? step.dice.filter((d) => d.hit).length : Object.values(step.hits).reduce((a, b) => a + b, 0);
  // Bombardment hits fall on everyone but the invader; Space Cannon hits fall on the invader only.
  const owners = new Set([...step.before, ...step.after].map((u) => u.owner));
  const targetSeats = [...owners].filter((o) =>
    step.kind === "bombardment" ? o !== invader : o === invader,
  );
  const targets = targetSeats
    .map((seat) => sideEffect(seat, step.before, step.after))
    .filter((side) => side.before.length > 0);
  const destroyed = targets.reduce((n, t) => n + t.lost.reduce((m, u) => m + u.count, 0), 0);
  const sustainedCount = targets.reduce((n, t) => n + t.sustained.reduce((m, u) => m + u.count, 0), 0);
  const absorbed = Math.max(0, hits - destroyed - sustainedCount) + sustainedCount;
  return { kind: step.kind, planet: step.planet, shooters, dice, hits, targets, destroyed, absorbed };
}

/** "2 Infantry, 1 Mech" */
export function formatCounts(counts: readonly UnitCount[], none = "none"): string {
  return counts.length
    ? counts.map(({ unit, count }) => `${count} ${getUnitDisplayName(unit, count)}`).join(", ")
    : none;
}

/** "Dreadnought, Bombardment 5 (3 dice)" -- the unit, the ability and its hit threshold. */
export function shooterLabel(kind: InvasionStepKind, group: DiceGroup): string {
  const ability = kind === "bombardment" ? "Bombardment" : "Space Cannon";
  const dice = group.faces.length;
  return `${getUnitDisplayName(group.unit)}, ${ability} ${group.target}${dice === 1 ? "" : ` x${dice}`}`;
}

/**
 * One plain sentence about the outcome, with seats as raw ids (callers resolve them: `name`
 * maps a seat to its display name, the default keeps the id for text that is resolved later).
 */
export function stepOutcomeText(
  summary: InvasionStepSummary,
  invader: string,
  name: (seat: string) => string = (seat) => seat,
): string {
  const planet = planetName(summary.planet);
  const title = STEP_TITLES[summary.kind];
  const rolled = summary.dice > 0 ? ` ${summary.hits} hit${summary.hits === 1 ? "" : "s"} from ${summary.dice} di${summary.dice === 1 ? "e" : "ce"}.` : "";
  if (summary.hits === 0) return `${title} on ${planet}: no hits.${summary.dice > 0 ? ` ${summary.dice} di${summary.dice === 1 ? "e" : "ce"} rolled, none reached the target.` : ""}`;
  const effects: string[] = [];
  for (const side of summary.targets) {
    const lost = side.lost.length ? `destroyed ${formatCounts(side.lost)}` : "";
    const sustained = side.sustained.length ? `${formatCounts(side.sustained)} sustained damage` : "";
    const parts = [lost, sustained].filter(Boolean);
    if (parts.length) effects.push(`${name(side.seat)}: ${parts.join(", ")}`);
  }
  const who = summary.kind === "bombardment" ? "" : ` against ${name(invader)}'s landed forces`;
  if (effects.length === 0)
    return `${title} on ${planet}:${rolled} No ground forces were lost${who}.`;
  return `${title} on ${planet}:${rolled} ${effects.join("; ")}.`;
}
