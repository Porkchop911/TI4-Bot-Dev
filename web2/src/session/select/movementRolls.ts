// What the dice of a movement did: the gravity rifts that ships left, and the space cannon offense
// after the movement. Read from the public rolls of the action (`GameEvent.roll`), which say who
// rolled, where, the faces, what a die needs, and the unit that rolled: the ship of a rift roll
// with its cargo, the gun of a space cannon roll and the seat that it fires at. Before the roll the
// space cannon offense is the guns of the movement facts (`MovementFacts.cannon`).
import type {
  BattleRowView,
  BattleSideView,
  BattleTableView,
  RiftRowView,
  UnitType,
} from "../../model";
import { plural } from "../../model";
import { reasonOf } from "../rolls";
import type { CannonFact, Catalog, PublicRoll } from "../wire";
import { byUnitOrder, humanizeId, unitKey, unitLabel, unitType } from "./names";

const RIFT = "gravity rift";
const CANNON = "space cannon";

/** The roll hit: a ship that leaves a rift survives on a hit. */
const hits = (roll: PublicRoll) => roll.hits ?? 0;

const nameOf = (unit: string, printed: Catalog | null, damaged = false, count = 1) =>
  unitLabel(unit, printed, count) + (damaged ? " (damaged)" : "");

/** "2 Infantry I, 1 Fighter II": units by the printed unit, counted, in the order of the units. */
function unitsText(units: readonly string[], printed: Catalog | null): string {
  const counts = new Map<string, { id: string; count: number }>();
  for (const unit of units) {
    const key = unitKey(unit, printed);
    counts.set(key, { id: unit, count: (counts.get(key)?.count ?? 0) + 1 });
  }
  return [...counts.values()]
    .sort((a, b) => byUnitOrder(a.id, b.id))
    .map(({ id, count }) => `${count} ${nameOf(id, printed, false, count)}`)
    .join(", ");
}

const riftRolls = (rolls: readonly PublicRoll[]) =>
  rolls.filter((roll) => reasonOf(roll) === RIFT && roll.faces.length > 0);

/**
 * One row for each rift roll of the action, in the order of the game: the ship, the rift that it
 * leaves, where it came from and what it carries. Null with none.
 */
export function riftRows(
  rolls: readonly PublicRoll[],
  /** The name of a system on the map. */
  label: (system: string) => string,
  printed: Catalog | null = null,
): RiftRowView[] | null {
  const rows = riftRolls(rolls).map((roll): RiftRowView => {
    const type = roll.unit ? unitType(roll.unit) : null;
    return {
      unit: type,
      name: roll.unit ? nameOf(roll.unit, printed, roll.damaged) : roll.label,
      text: [
        roll.system ? `Leaves ${label(roll.system)}` : null,
        roll.origin && roll.origin !== roll.system ? `from ${label(roll.origin)}` : null,
        roll.cargo?.length ? `carries ${unitsText(roll.cargo, printed)}` : null,
        roll.hits_on ? `survives on ${roll.hits_on}+` : null,
      ]
        .filter((part) => part !== null)
        .join(" · "),
      roll: { face: roll.faces[0], lost: hits(roll) === 0 },
    };
  });
  return rows.length ? rows : null;
}

/** The names of the ships that a rift destroyed, each with its cargo, in the order of the game. */
export function removedUnits(
  rolls: readonly PublicRoll[],
  printed: Catalog | null = null,
): string[] {
  return riftRolls(rolls)
    .filter((roll) => hits(roll) === 0)
    .flatMap((roll) => [roll.unit ?? "", ...(roll.cargo ?? [])])
    .filter((unit) => unitType(unit) !== null)
    .map((unit) => unitLabel(unit, printed));
}

/** The ships of a seat in the active system that the guns fire at, with their printed names. */
export type ShipsOf = (seat: string) => { unit: UnitType; name?: string; count: number }[];

const row = (change: Partial<BattleRowView>): BattleRowView => ({
  unit: null,
  count: null,
  target: null,
  targetTitle: "",
  modified: false,
  dice: [],
  hits: "",
  scored: false,
  pills: [],
  gone: false,
  assign: [],
  ...change,
});

/** The row of one gun, or of an attachment's SPACE CANNON. */
const gunName = (
  unit: string | null | undefined,
  planet: string | null | undefined,
  printed: Catalog | null,
) =>
  unit
    ? { unit: unitType(unit), label: nameOf(unit, printed) }
    : { unit: null, label: planet ? `Space cannon · ${humanizeId(planet)}` : "Space cannon" };

function rolledRow(roll: PublicRoll, printed: Catalog | null): BattleRowView {
  const target = roll.hits_on ?? null;
  return row({
    ...gunName(roll.unit, null, printed),
    target,
    targetTitle: target ? `Hits on ${target}+` : "",
    dice: roll.faces.map((face) => ({ roll: face, hit: target !== null && face >= target })),
    hits: target ? plural(hits(roll), "hit") : "",
    scored: hits(roll) > 0,
    pills: roll.rerolled?.length ? [{ tone: "staged", label: "Reroll" }] : [],
  });
}

const shipRows = (ships: ShipsOf, seat: string) =>
  ships(seat).map((ship) => row({ unit: ship.unit, label: ship.name, count: ship.count }));

/**
 * The table of seats: the seat of the action first. A seat that fires has its guns; a seat that is
 * fired at has its ships.
 */
function table(
  fired: { seat: string; target: string | null; rows: BattleRowView[]; hits: number | null }[],
  active: string,
  viewer: string,
  ships: ShipsOf,
  score: string,
): BattleTableView {
  const targets = new Set(fired.flatMap((gun) => (gun.target ? [gun.target] : [])));
  const seats = [...new Set([...fired.map((gun) => gun.seat), ...targets])].sort(
    (a, b) => Number(b === active) - Number(a === active),
  );
  const sides = seats.map((seat): BattleSideView => {
    const own = fired.filter((gun) => gun.seat === seat);
    const scored = own.map((gun) => gun.hits).filter((count) => count !== null);
    return {
      seat,
      role: seat === active ? "Attacker" : "Defender",
      isViewer: seat === viewer,
      hits: own.length && scored.length ? scored.reduce((sum, count) => sum + count, 0) : null,
      meta: [],
      cards: "",
      rows: [
        ...own.flatMap((gun) => gun.rows),
        ...(targets.has(seat) ? shipRows(ships, seat) : []),
      ],
    };
  });
  return { label: "Space cannon offense", score, owed: null, sides, notes: [] };
}

/** Whom a gun fires at, when the roll does not say: every gun but the seat's own fires at it. */
const targetOf = (seat: string, target: string | null | undefined, active: string) =>
  target ?? (seat === active ? null : active);

/**
 * The space cannon offense of the action: a side for each seat that fired, a row for each gun, and
 * the ships that they fired at. `active` is the seat of the action. Null when nothing fired. A
 * space cannon roll always names its seat; one that does not is left out.
 */
export function cannonTable(
  rolls: readonly PublicRoll[],
  active: string,
  viewer: string,
  ships: ShipsOf = () => [],
  printed: Catalog | null = null,
): BattleTableView | null {
  const fired = rolls.filter(
    (roll) => reasonOf(roll) === CANNON && roll.faces.length > 0 && roll.roller,
  );
  if (!fired.length) {
    return null;
  }
  const total = fired.reduce((sum, roll) => sum + hits(roll), 0);
  return table(
    fired.map((roll) => ({
      seat: roll.roller!,
      target: targetOf(roll.roller!, roll.target, active),
      rows: [rolledRow(roll, printed)],
      hits: hits(roll),
    })),
    active,
    viewer,
    ships,
    plural(total, "hit"),
  );
}

/** "≈0.5": the hits that `dice` dice that hit on `target` score on average. */
const expected = (dice: number, target: number) =>
  `≈${((dice * (11 - Math.min(Math.max(target, 1), 11))) / 10).toFixed(1)}`;

/**
 * The space cannon offense before it is rolled: the guns of the movement facts, one row for the
 * guns of a kind of a seat with the dice that they will roll, and the ships that they fire at.
 * Null when no gun fires.
 */
export function cannonPreview(
  guns: readonly CannonFact[],
  active: string,
  viewer: string,
  ships: ShipsOf,
  printed: Catalog | null = null,
): BattleTableView | null {
  if (!guns.length) {
    return null;
  }
  const groups = new Map<string, { gun: CannonFact; count: number }>();
  for (const gun of guns) {
    const key = [gun.seat, gun.unit, gun.planet, gun.hits_on, gun.dice, gun.target].join("|");
    const group = groups.get(key);
    groups.set(key, { gun, count: (group?.count ?? 0) + 1 });
  }
  const dice = guns.reduce((sum, gun) => sum + gun.dice, 0);
  return table(
    [...groups.values()].map(({ gun, count }) => ({
      seat: gun.seat,
      target: targetOf(gun.seat, gun.target, active),
      rows: [
        row({
          ...gunName(gun.unit, gun.planet, printed),
          count,
          target: gun.hits_on,
          targetTitle: `Hits on ${gun.hits_on}+`,
          dice: count * gun.dice,
          hits: expected(count * gun.dice, gun.hits_on),
        }),
      ],
      hits: null,
    })),
    active,
    viewer,
    ships,
    plural(dice, "die", "dice"),
  );
}
