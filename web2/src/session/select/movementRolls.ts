// What the dice of a movement did: the gravity rifts that ships left, and the space cannon offense
// after the movement. Read from the public rolls of the action (`GameEvent.roll`), which say who
// rolled, where, the faces and what a die needs. They do not name the ship or the unit that
// rolled, so neither do these rows.
import type { BattleRowView, BattleSideView, BattleTableView, RiftRowView } from "../../model";
import { plural } from "../../model";
import { reasonOf } from "../rolls";
import type { PublicRoll } from "../wire";

const RIFT = "gravity rift";
const CANNON = "space cannon";

/** The roll hit: a ship that leaves a rift survives on a hit. */
const hits = (roll: PublicRoll) => roll.hits ?? 0;

/** One row for each rift roll of the action, in the order of the game. Null with none. */
export function riftRows(
  rolls: readonly PublicRoll[],
  /** The name of a system on the map. */
  label: (system: string) => string,
): RiftRowView[] | null {
  const rows = rolls
    .filter((roll) => reasonOf(roll) === RIFT && roll.faces.length > 0)
    .map((roll): RiftRowView => ({
      unit: null,
      name: roll.system ? label(roll.system) : roll.label,
      text: roll.hits_on ? `Survives on ${roll.hits_on}+` : "",
      roll: { face: roll.faces[0], lost: hits(roll) === 0 },
    }));
  return rows.length ? rows : null;
}

function cannonRow(roll: PublicRoll): BattleRowView {
  const target = roll.hits_on ?? null;
  return {
    unit: null,
    label: roll.label,
    count: null,
    target,
    targetTitle: target ? `Hits on ${target}+` : "",
    modified: false,
    dice: roll.faces.map((face) => ({ roll: face, hit: target !== null && face >= target })),
    hits: target ? plural(hits(roll), "hit") : "",
    scored: hits(roll) > 0,
    pills: roll.rerolled?.length ? [{ tone: "staged", label: "Reroll" }] : [],
    gone: false,
    assign: [],
  };
}

/**
 * The space cannon offense of the action: a side for each seat that fired, a row for each roll.
 * `active` is the seat of the action: its own guns fire at the other side. Null when nothing fired.
 * A space cannon roll always names its seat; one that does not is left out.
 */
export function cannonTable(
  rolls: readonly PublicRoll[],
  active: string,
  viewer: string,
): BattleTableView | null {
  const fired = rolls.filter(
    (roll) => reasonOf(roll) === CANNON && roll.faces.length > 0 && roll.roller,
  );
  if (!fired.length) {
    return null;
  }
  const seats = [...new Set(fired.map((roll) => roll.roller!))].sort(
    (a, b) => Number(b === active) - Number(a === active),
  );
  const sides = seats.map((seat): BattleSideView => {
    const own = fired.filter((roll) => roll.roller === seat);
    return {
      seat,
      role: seat === active ? "Attacker" : "Defender",
      isViewer: seat === viewer,
      hits: own.reduce((sum, roll) => sum + hits(roll), 0),
      meta: [],
      cards: "",
      rows: own.map(cannonRow),
    };
  });
  const total = sides.reduce((sum, side) => sum + (side.hits ?? 0), 0);
  return {
    label: "Space cannon offense",
    score: plural(total, "hit"),
    owed: null,
    sides,
    notes: [],
  };
}
