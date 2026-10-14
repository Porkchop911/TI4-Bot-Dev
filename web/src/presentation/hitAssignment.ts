import type { ChoiceOptionDto, PlacedUnitView } from "../protocol/types.ts";
import { getUnitBaseType } from "../components/UnitIcon.tsx";

/** Standard base transport capacity per ship type in TI4. */
export function getBaseCapacity(unitType: string): number {
  switch (getUnitBaseType(unitType)) {
    case "carrier":
      return 4;
    case "warsun":
      return 6;
    case "dreadnought":
      return 1;
    case "flagship":
      return 3;
    default:
      return 0;
  }
}

/** Returns true if the unit is transported / requires capacity in space. */
export function requiresCapacity(unitType: string): boolean {
  const base = getUnitBaseType(unitType);
  return base === "fighter" || base === "infantry" || base === "mech";
}

function isGroundForce(unitType: string): boolean {
  const base = getUnitBaseType(unitType);
  return base === "infantry" || base === "mech";
}

/** A step of a casualty batch plan, as the server's `casualties` batch kind takes it. */
export type CasualtyStep =
  | { kind: "sustain"; unit: string }
  | { kind: "destroy"; unit: string; damaged: boolean };

/**
 * One line of the hit-assignment panel. Ships whose choice matters (they can sustain damage, or
 * they carry cargo) are a row each; everything else is grouped by type and damage.
 */
export interface HitRow {
  key: string;
  unitType: string;
  damaged: boolean;
  count: number;
  individual: boolean;
  /** Can take this hit by sustaining damage instead of being destroyed. */
  canSustain: boolean;
  capacity: number;
}

/** Staged hits per row: ships destroyed, and (individual rows) whether the ship sustains. */
export type HitStaging = Record<string, { destroy: number; sustain: boolean }>;

export interface HitContext {
  /** The deciding seat's units that can take these hits. */
  units: PlacedUnitView[];
  /** Unit types offered for sustaining in the current decision. */
  sustainTypes: Set<string>;
  /** (type, damaged) pairs the engine offers to destroy, when the decision lists them. */
  destroyable?: Set<string> | null;
  /** Anti-fighter barrage: only fighters can be hit. */
  onlyFighters?: boolean;
}

export const destroyKey = (unitType: string, damaged: boolean) =>
  `${unitType}|${damaged ? "damaged" : "intact"}`;

/** The units that can take hits in a space combat: the seat's ships in space. */
export function spaceHitUnits(
  units: PlacedUnitView[],
  seat: string,
): PlacedUnitView[] {
  return units.filter(
    (u) => u.owner === seat && !u.planet && !isGroundForce(u.unit_type),
  );
}

/** The units that can take hits in a ground combat: the seat's ground forces on the planet. */
export function groundHitUnits(
  units: PlacedUnitView[],
  seat: string,
  planet: string,
): PlacedUnitView[] {
  return units.filter(
    (u) =>
      u.owner === seat && u.planet === planet && isGroundForce(u.unit_type),
  );
}

/** Fighters and ground forces in space that need transport capacity. */
export function spaceCargo(units: PlacedUnitView[], seat: string): number {
  return units.filter(
    (u) => u.owner === seat && !u.planet && requiresCapacity(u.unit_type),
  ).length;
}

/**
 * Cargo the staged losses would leave without transport capacity (removed by the capacity rule,
 * LRR 16): fighters lost to hits no longer need room, lost capacity ships take theirs away.
 */
export function strandedCargo(
  rows: HitRow[],
  staging: HitStaging,
  cargo: number,
  capacity: number,
): number {
  const fightersLost = rows
    .filter((row) => getUnitBaseType(row.unitType) === "fighter")
    .reduce((sum, row) => sum + (staging[row.key]?.destroy ?? 0), 0);
  const before = Math.max(0, cargo - capacity);
  const after = Math.max(
    0,
    cargo - fightersLost - (capacity - lostCapacity(rows, staging)),
  );
  return Math.max(0, after - before);
}

export function buildHitRows(ctx: HitContext): HitRow[] {
  const rows: HitRow[] = [];
  const groups = new Map<string, HitRow>();
  const counters = new Map<string, number>();
  for (const unit of ctx.units) {
    if (ctx.onlyFighters && getUnitBaseType(unit.unit_type) !== "fighter")
      continue;
    const capacity = getBaseCapacity(unit.unit_type);
    const canSustain = !unit.damaged && ctx.sustainTypes.has(unit.unit_type);
    // Only ships that can sustain ever carry damage, so a damaged ship is one of them too.
    if (canSustain || capacity > 0 || unit.damaged) {
      const n = (counters.get(unit.unit_type) ?? 0) + 1;
      counters.set(unit.unit_type, n);
      rows.push({
        key: `${unit.unit_type}#${n}${unit.damaged ? "d" : ""}`,
        unitType: unit.unit_type,
        damaged: unit.damaged,
        count: 1,
        individual: true,
        canSustain,
        capacity,
      });
      continue;
    }
    const key = destroyKey(unit.unit_type, unit.damaged);
    const group = groups.get(key);
    if (group) group.count += 1;
    else {
      const row: HitRow = {
        key,
        unitType: unit.unit_type,
        damaged: unit.damaged,
        count: 1,
        individual: false,
        canSustain: false,
        capacity,
      };
      groups.set(key, row);
      rows.push(row);
    }
  }
  return rows;
}

/**
 * Whether a row's ships may be destroyed in this decision. An undamaged sustaining ship is
 * destroyed only after it sustained, so the engine then offers it as a damaged ship.
 */
export function canDestroy(row: HitRow, ctx: HitContext): boolean {
  return (
    !ctx.destroyable ||
    ctx.destroyable.has(
      destroyKey(row.unitType, row.canSustain ? true : row.damaged),
    )
  );
}

export function stagedHits(staging: HitStaging): number {
  return Object.values(staging).reduce(
    (sum, s) => sum + s.destroy + (s.sustain ? 1 : 0),
    0,
  );
}

/**
 * Hits a row can take. A ship that is already damaged, or cannot sustain, takes one hit to
 * destroy; an undamaged sustaining ship takes two: one to sustain damage, one to destroy it.
 */
export function rowHits(row: HitRow, ctx: HitContext): number {
  if (row.canSustain) return 1 + (canDestroy(row, ctx) ? 1 : 0);
  return canDestroy(row, ctx) ? row.count : 0;
}

/** Hits that can be assigned at all across the rows. */
export function assignableHits(rows: HitRow[], ctx: HitContext): number {
  return rows.reduce((sum, row) => sum + rowHits(row, ctx), 0);
}

/** The number of hits the plan must cover before it can be confirmed. */
export function hitsToCover(
  hits: number,
  rows: HitRow[],
  ctx: HitContext,
): number {
  return Math.min(hits, assignableHits(rows, ctx));
}

/** Whether the panel can take any hit here; when it cannot, the per-click controls must stay. */
export function canStageHits(ctx: HitContext): boolean {
  return assignableHits(buildHitRows(ctx), ctx) > 0;
}

export function canAddDestroy(
  row: HitRow,
  staging: HitStaging,
  hits: number,
  ctx: HitContext,
): boolean {
  const staged = staging[row.key] ?? { destroy: 0, sustain: false };
  if (!canDestroy(row, ctx) || staged.destroy >= row.count) return false;
  // An undamaged sustaining ship sustains first, so destroying it takes a second hit.
  const cost = row.canSustain && !staged.sustain ? 2 : 1;
  return stagedHits(staging) + cost <= hits;
}

export function canSustain(
  row: HitRow,
  staging: HitStaging,
  hits: number,
): boolean {
  const staged = staging[row.key] ?? { destroy: 0, sustain: false };
  return row.canSustain && !staged.sustain && stagedHits(staging) < hits;
}

/** Stages one destroy; on an undamaged sustaining ship that stages its sustain too. */
export function addDestroy(staging: HitStaging, row: HitRow): HitStaging {
  const staged = staging[row.key] ?? { destroy: 0, sustain: false };
  return {
    ...staging,
    [row.key]: {
      destroy: staged.destroy + 1,
      sustain: staged.sustain || row.canSustain,
    },
  };
}

/** Takes back the last hit on a row: the destroy first, then the sustain it followed. */
export function removeHit(staging: HitStaging, row: HitRow): HitStaging {
  const staged = staging[row.key];
  if (!staged) return staging;
  const next =
    staged.destroy > 0
      ? { ...staged, destroy: staged.destroy - 1 }
      : { ...staged, sustain: false };
  return { ...staging, [row.key]: next };
}

export function addSustain(staging: HitStaging, row: HitRow): HitStaging {
  const staged = staging[row.key] ?? { destroy: 0, sustain: false };
  return { ...staging, [row.key]: { ...staged, sustain: true } };
}

/** Rough cost of losing a ship, cheapest first, for filling the remaining hits automatically. */
const LOSS_ORDER = [
  "fighter",
  "infantry",
  "destroyer",
  "cruiser",
  "mech",
  "carrier",
  "dreadnought",
  "flagship",
  "warsun",
];

/**
 * Fill the remaining hits the way most players would: sustain first, then lose the cheapest
 * ships, keeping capacity ships as long as possible.
 */
export function autoFill(
  rows: HitRow[],
  staging: HitStaging,
  hits: number,
  ctx: HitContext,
): HitStaging {
  let next = staging;
  for (const row of rows) {
    if (stagedHits(next) >= hits) return next;
    if (canSustain(row, next, hits)) next = addSustain(next, row);
  }
  const order = [...rows].sort(
    (a, b) =>
      LOSS_ORDER.indexOf(getUnitBaseType(a.unitType)) -
        LOSS_ORDER.indexOf(getUnitBaseType(b.unitType)) ||
      Number(b.damaged) - Number(a.damaged),
  );
  for (const row of order) {
    while (canAddDestroy(row, next, hits, ctx)) next = addDestroy(next, row);
  }
  return next;
}

/**
 * The plan sent to the server: every sustain first, then one destroy per lost ship. The engine
 * asks hit by hit, sustain before loss; the server answers "take the hit" itself before a destroy.
 * A ship that sustains and is then destroyed is lost as a damaged ship.
 */
export function casualtyPlan(
  rows: HitRow[],
  staging: HitStaging,
): CasualtyStep[] {
  const steps: CasualtyStep[] = [];
  for (const row of rows)
    if (staging[row.key]?.sustain)
      steps.push({ kind: "sustain", unit: row.unitType });
  for (const row of rows) {
    const damaged = row.damaged || (row.canSustain && !!staging[row.key]?.sustain);
    for (let i = 0; i < (staging[row.key]?.destroy ?? 0); i++)
      steps.push({ kind: "destroy", unit: row.unitType, damaged });
  }
  return steps;
}

/** Capacity the staged losses take away. */
export function lostCapacity(rows: HitRow[], staging: HitStaging): number {
  return rows.reduce(
    (sum, row) => sum + row.capacity * (staging[row.key]?.destroy ?? 0),
    0,
  );
}

/** (type, damaged) pairs an assign-casualty decision offers, or null when it lists none. */
export function destroyableFromOptions(
  options: ChoiceOptionDto[],
): Set<string> | null {
  const pairs = options
    .filter((o) => o.kind === "casualty" || o.kind === "ground_casualty")
    .flatMap((o) =>
      typeof o.payload?.unit === "string"
        ? [destroyKey(o.payload.unit, o.payload.damaged === true)]
        : [],
    );
  return pairs.length ? new Set(pairs) : null;
}

/**
 * What a sustain decision lets the seat lose afterwards, or null when it does not say. The engine
 * marks "take the hit" `non_fighters_first` when the hits must go to non-fighter ships while any
 * are left (Graviton Laser System), so a planned loss never names a fighter it will not offer.
 */
export function sustainDestroyableFromOptions(
  options: ChoiceOptionDto[],
  units: PlacedUnitView[],
): Set<string> | null {
  const bound = options.some(
    (o) => o.kind === "decline" && o.payload?.non_fighters_first === true,
  );
  const ships = units.filter(
    (u) => getUnitBaseType(u.unit_type) !== "fighter",
  );
  if (!bound || ships.length === 0) return null;
  // Both damage states: a ship that sustains first is then lost as a damaged ship.
  return new Set(
    ships.flatMap((u) => [
      destroyKey(u.unit_type, false),
      destroyKey(u.unit_type, true),
    ]),
  );
}

/** Unit types a sustain decision offers. */
export function sustainTypesFromOptions(
  options: ChoiceOptionDto[],
): Set<string> {
  return new Set(
    options
      .filter((o) => o.kind === "sustain")
      .flatMap((o) =>
        typeof o.payload?.unit === "string" ? [o.payload.unit] : [],
      ),
  );
}

/** Whether every unit a casualty decision offers is a fighter (an anti-fighter barrage's hits). */
export function onlyFighterOptions(options: ChoiceOptionDto[]): boolean {
  const units = options.flatMap((o) =>
    (o.kind === "casualty" || o.kind === "ground_casualty") &&
    typeof o.payload?.unit === "string"
      ? [o.payload.unit]
      : [],
  );
  return (
    units.length > 0 && units.every((u) => getUnitBaseType(u) === "fighter")
  );
}
