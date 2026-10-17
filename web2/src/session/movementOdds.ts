// The space combat that a staged movement starts: what the engine of the page is asked for its
// odds. The engine says what the state has in the active system (`MovementFacts.battle`); the
// ships that the movement brings are added here, with the fighters that they carry.
import { type MovementDraft, shipId } from "./movementDraft";
import { isShip, unitType } from "./select/names";
import type { BattleResponse, CombatSide, MovementFacts } from "./wire";

/**
 * A question for `Transport.odds`. The key is its JSON: the same sides have the same key. The
 * sides are as `MovementFacts.battle` has them, a narrower `SpaceOddsRequest`.
 */
export interface OddsRequest {
  key: string;
  request: { attacker: CombatSide; defender: CombatSide };
}

/** The last answer of `Transport.odds`, with the key of its question. Null: no odds for it. */
export interface KeptOdds {
  key: string;
  response: BattleResponse | null;
}

/** A count by unit id, with its keys in order, so that the same counts are the same JSON. */
function sorted(counts: Record<string, number>): Record<string, number> {
  return Object.fromEntries(
    Object.entries(counts)
      .filter(([, count]) => count > 0)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  );
}

function sideOf(side: CombatSide, units = side.units ?? {}, damaged = side.damaged ?? {}) {
  const out: CombatSide = { faction: side.faction };
  for (const [key, counts] of [
    ["units", units],
    ["damaged", damaged],
    ["guns", side.guns ?? {}],
  ] as const) {
    const kept = sorted(counts);
    if (Object.keys(kept).length) {
      out[key] = kept;
    }
  }
  return out;
}

/**
 * The odds that the staged movement asks for: the attacker with the ships that move in and the
 * fighters in their holds, against the defender. Null when no other seat has ships in the active
 * system, or when no ship moves.
 */
export function oddsRequest(facts: MovementFacts, staged: MovementDraft): OddsRequest | null {
  const battle = facts.battle;
  if (!battle) {
    return null;
  }
  const units = { ...battle.attacker.units };
  const damaged = { ...battle.attacker.damaged };
  const add = (into: Record<string, number>, unit: string, count: number) => {
    into[unit] = (into[unit] ?? 0) + count;
  };
  let arrives = 0;
  for (const ship of facts.ships) {
    const hold = staged[shipId(ship)];
    if (!hold) {
      continue;
    }
    arrives += 1;
    add(units, ship.unit, 1);
    if (ship.damaged) {
      add(damaged, ship.unit, 1);
    }
    // Ground forces in a hold do not fight in space.
    for (const [pool, count] of Object.entries(hold)) {
      const cargo = facts.cargo[Number(pool)];
      const type = cargo && unitType(cargo.unit);
      if (type && isShip(type)) {
        add(units, cargo.unit, count);
        if (cargo.damaged) {
          add(damaged, cargo.unit, count);
        }
      }
    }
  }
  if (!arrives) {
    return null;
  }
  const request: OddsRequest["request"] = {
    attacker: sideOf(battle.attacker, units, damaged),
    defender: sideOf(battle.defender_side),
  };
  return { key: JSON.stringify(request), request };
}
