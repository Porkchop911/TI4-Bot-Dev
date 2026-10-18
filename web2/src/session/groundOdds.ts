// The ground combats that the staged landings start: what the engine of the page is asked for
// their odds. The engine says, for each planet of the active system, who defends it and what
// the odds can count (`InvasionView.odds_context`); the units that land are added here.
import type { LandingDraft } from "./landingDraft";
import type { GroundOddsRequest, GroundOddsResponse, InvasionFacts, SessionUpdate } from "./wire";

/** A question for `Transport.groundOdds`. The key is its JSON: the same sides have the same key. */
export interface GroundOddsAsk {
  planet: string;
  /** The seat that defends the planet. */
  defender: string;
  key: string;
  request: GroundOddsRequest;
}

/** The last answer for a planet, with the key of its question. Null: no odds for it. */
export interface KeptGroundOdds {
  key: string;
  response: GroundOddsResponse | null;
}

/** What the odds of a planet are, before the engine is asked. */
export type GroundAsk =
  | { planet: string; kind: "ask"; ask: GroundOddsAsk }
  /** A defense of the planet that the odds cannot count. */
  | { planet: string; kind: "unavailable"; defender: string };

/** A count by unit id, with its keys in order, so that the same counts are the same JSON. */
function sorted(counts: Record<string, number>): Record<string, number> {
  return Object.fromEntries(
    Object.entries(counts)
      .filter(([, count]) => count > 0)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  );
}

/** The side of a seat: its units, and the damaged ones when there are. */
function side(faction: string, units: Record<string, number>, damaged: Record<string, number>) {
  const hurt = sorted(damaged);
  return {
    faction,
    units: sorted(units),
    ...(Object.keys(hurt).length ? { damaged: hurt } : {}),
  };
}

/**
 * The odds that the staged landings ask for, one for each planet where a unit lands and another
 * seat has ground forces: the ground forces of the viewer there and those that land, against the
 * defender with its guns, and the ships that bombard during the combat.
 */
export function groundAsks(
  update: SessionUpdate,
  facts: InvasionFacts,
  draft: LandingDraft,
): GroundAsk[] {
  const { board, players } = update.view;
  const contexts = board.invasion?.odds_context ?? {};
  const invader = board.invasion?.invader;
  const faction = (seat: string) => players.find((player) => player.id === seat)?.faction;
  const there = board.systems[facts.active]?.units ?? [];
  const asks: GroundAsk[] = [];
  for (const { planet } of facts.planets) {
    const lands = Object.entries(draft[planet] ?? {}).filter(([, count]) => count > 0);
    const context = contexts[planet];
    if (!lands.length || !context?.opponent || !invader) {
      continue;
    }
    const defender = context.opponent;
    if (!context.available) {
      asks.push({ planet, kind: "unavailable", defender });
      continue;
    }
    const attack = faction(invader);
    const defend = faction(defender);
    if (!attack || !defend) {
      continue;
    }
    const ground = new Set(context.ground_force_types);
    const count = (owner: string) => {
      const units: Record<string, number> = {};
      const damaged: Record<string, number> = {};
      for (const unit of there) {
        if (unit.planet === planet && unit.owner === owner && ground.has(unit.unit_type)) {
          units[unit.unit_type] = (units[unit.unit_type] ?? 0) + 1;
          if (unit.damaged) {
            damaged[unit.unit_type] = (damaged[unit.unit_type] ?? 0) + 1;
          }
        }
      }
      return { units, damaged };
    };
    const mine = count(invader);
    for (const [force, number] of lands) {
      const pool = facts.forces[Number(force)];
      if (!pool) {
        continue;
      }
      mine.units[pool.unit] = (mine.units[pool.unit] ?? 0) + number;
      if (pool.damaged) {
        mine.damaged[pool.unit] = (mine.damaged[pool.unit] ?? 0) + number;
      }
    }
    const theirs = count(defender);
    const guns = sorted(context.additional_guns);
    const harrow = sorted(context.harrow_units);
    const request: GroundOddsRequest = {
      attacker: side(attack, mine.units, mine.damaged),
      defender: {
        ...side(defend, theirs.units, theirs.damaged),
        ...(Object.keys(guns).length ? { guns } : {}),
      },
      ...(Object.keys(harrow).length ? { harrow } : {}),
    };
    asks.push({
      planet,
      kind: "ask",
      ask: { planet, defender, key: JSON.stringify(request), request },
    });
  }
  return asks;
}
