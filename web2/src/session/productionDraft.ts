// The staged production of one use of PRODUCTION: the purchases, in the order that the player
// made them, and what pays for them. Nothing of it is sent before "Produce units". The prices,
// the places and the limits are facts of the engine; this file only adds up what is staged.
import type { PlanStep, ProductionStep } from "./movementPlan";
import { COMMODITY, GOODS, exhaustId, leastWaste } from "./select/payment";
import type { FleetCounts, FleetStanding, ProductionFacts } from "./wire";

export { GOODS } from "./select/payment";

export interface ProductionDraft {
  /** One purchase each: a unit of the facts, and the place of what it makes. */
  builds: { unit: number; place: string }[];
  /** The planets that are exhausted for the payment, in the order that they were chosen. */
  planets: string[];
  /** The trade goods that are spent. */
  goods: number;
  /** The commodities that are spent as trade goods (the Keleres agent). */
  commodities?: number;
}

export const EMPTY: ProductionDraft = { builds: [], planets: [], goods: 0, commodities: 0 };

export const isEmpty = (draft: ProductionDraft) =>
  draft.builds.length === 0 &&
  draft.planets.length === 0 &&
  draft.goods === 0 &&
  !draft.commodities;

/** The production limit that is left before any staged purchase. */
const room = (facts: ProductionFacts) => Math.max(0, facts.capacity.limit - facts.capacity.used);

/**
 * How many units each staged purchase makes. A purchase of two makes one when the production
 * limit has room for one only, at the same price. The limit is soft: a purchase after the room
 * is used makes all its units, and `overrun` says that the build is over.
 */
export function sizes(facts: ProductionFacts, draft: ProductionDraft): number[] {
  let left = room(facts);
  return draft.builds.map((build) => {
    const per = facts.units[build.unit]?.per ?? 0;
    if (left <= 0) {
      return per;
    }
    const made = Math.min(per, left);
    left -= made;
    return made;
  });
}

export interface Totals {
  /** The units that the staged purchases make. */
  units: number;
  /** The production limit that is left for them. */
  capacity: number;
  /** What the purchases cost, after the discount and what is already paid. */
  cost: number;
  /** What the staged planets and trade goods pay. */
  paid: number;
  /** What the ready planets, the trade goods and the commodities of the seat can pay. */
  available: number;
  /** The staged units of each unit of the facts. */
  made: number[];
}

export function totals(facts: ProductionFacts, draft: ProductionDraft): Totals {
  const size = sizes(facts, draft);
  const made = facts.units.map(() => 0);
  let printed = 0;
  draft.builds.forEach((build, index) => {
    const unit = facts.units[build.unit];
    if (!unit) {
      return;
    }
    made[build.unit] += size[index];
    printed += unit.cost;
  });
  const worth = new Map(facts.planets.map((planet) => [planet.planet, planet.worth]));
  return {
    units: size.reduce((sum, count) => sum + count, 0),
    capacity: room(facts),
    cost: facts.free ? 0 : Math.max(0, Math.max(0, printed - facts.discount) - facts.credit),
    paid:
      draft.planets.reduce((sum, planet) => sum + (worth.get(planet) ?? 0), 0) +
      (draft.goods + (draft.commodities ?? 0)) * facts.trade_good_worth,
    available:
      facts.planets.reduce((sum, planet) => sum + planet.worth, 0) +
      (facts.trade_goods + (facts.commodities ?? 0)) * facts.trade_good_worth,
    made,
  };
}

/**
 * The counts of the fleet supply and the capacity in the system with the staged units added: a
 * ship that counts against the fleet pool, the capacity of a ship, and a fighter or a ground
 * force in the space area. The engine reads them (`Transport.fleetStanding`).
 */
export function stagedCounts(facts: ProductionFacts, draft: ProductionDraft): FleetCounts {
  const size = sizes(facts, draft);
  const counts = { ...facts.fleet };
  draft.builds.forEach((build, index) => {
    const unit = facts.units[build.unit];
    if (!unit || build.place !== "space") {
      return;
    }
    const made = size[index];
    counts.ships += unit.fleet ? made : 0;
    counts.transport += unit.capacity * made;
    if (unit.fighter) {
      counts.fighters += made;
    } else if (unit.cargo) {
      counts.carried += made;
    }
  });
  return counts;
}

/** The last answer of the engine for the counts of a build, with the key of its question. */
export interface KeptStanding {
  key: string;
  response: FleetStanding | null;
}

/** Why a unit cannot be produced in this use, in words. */
export const UNAVAILABLE: Record<
  NonNullable<ProductionFacts["units"][number]["unavailable"]>,
  string
> = {
  technology: "Needs its technology",
  supply: "None left in supply",
  blockade: "Ships of another player are here",
  no_place: "No place for it here",
  only_another: "This use of PRODUCTION makes another unit only",
  too_costly: "This use of PRODUCTION makes cheaper units only",
  banned: "An ability of yours bars it from a production by an ability",
  barred: "An ability of yours bars your producers here from making it",
  not_offered: "The game does not offer it here",
};

/**
 * Why one more purchase of a unit cannot be staged. Null when it can. The supply of the units
 * (the plastic) is a hard limit. The resources and the production limit are soft: a build over
 * them is staged, the gauges say so, and the main button waits (`overrun`).
 */
export function blocked(
  facts: ProductionFacts,
  draft: ProductionDraft,
  unit: number,
): string | null {
  const total = totals(facts, draft);
  const kind = facts.units[unit];
  if (!kind) {
    return "Cannot be produced";
  }
  if (kind.unavailable) {
    return UNAVAILABLE[kind.unavailable];
  }
  if (kind.supply !== undefined && total.made[unit] >= kind.supply) {
    return "None left in supply";
  }
  return null;
}

/** What the staged build asks beyond its soft limits. None of it can be sent. */
export interface Overrun {
  /** The resources that the seat does not have. */
  short: number;
  /** The units beyond the production limit. */
  capacity: number;
}

export function overrun(facts: ProductionFacts, draft: ProductionDraft): Overrun {
  const total = totals(facts, draft);
  return {
    short: total.units > 0 ? Math.max(0, total.cost - total.available) : 0,
    capacity: Math.max(0, total.units - total.capacity),
  };
}

/** Whether the build is over any limit, as one number: the main button waits while it is above 0. */
export function overrunOf(facts: ProductionFacts, draft: ProductionDraft): number {
  const over = overrun(facts, draft);
  return over.short + over.capacity;
}

/** One more purchase of a unit, at one of its places. */
export function addBuild(
  facts: ProductionFacts,
  draft: ProductionDraft,
  unit: number,
  place: string,
): ProductionDraft {
  return blocked(facts, draft, unit) === null && facts.units[unit].places.includes(place)
    ? { ...draft, builds: [...draft.builds, { unit, place }] }
    : draft;
}

export const removeBuild = (draft: ProductionDraft, index: number): ProductionDraft => ({
  ...draft,
  builds: draft.builds.filter((_, at) => at !== index),
});

/** A planet pays, or no longer pays. */
export function togglePlanet(
  facts: ProductionFacts,
  draft: ProductionDraft,
  planet: string,
): ProductionDraft {
  if (draft.planets.includes(planet)) {
    return { ...draft, planets: draft.planets.filter((item) => item !== planet) };
  }
  return facts.planets.some((item) => item.planet === planet)
    ? { ...draft, planets: [...draft.planets, planet] }
    : draft;
}

/** The trade goods that pay, or the commodities (`source` is the id of their counter). */
export const setGoods = (
  facts: ProductionFacts,
  draft: ProductionDraft,
  value: number,
  source: string = GOODS,
): ProductionDraft =>
  source === COMMODITY
    ? { ...draft, commodities: Math.max(0, Math.min(value, facts.commodities ?? 0)) }
    : source === GOODS
      ? { ...draft, goods: Math.max(0, Math.min(value, facts.trade_goods)) }
      : draft;

export const clearPayment = (draft: ProductionDraft): ProductionDraft => ({
  ...draft,
  planets: [],
  goods: 0,
  commodities: 0,
});

/**
 * The payment with the least waste: planets before trade goods, and of two payments with the
 * same waste the one with fewer planets.
 */
export function autoPay(facts: ProductionFacts, draft: ProductionDraft): ProductionDraft {
  const pick = leastWaste(
    facts.planets.map((planet) => ({ id: planet.planet, worth: planet.worth })),
    facts.trade_goods,
    facts.commodities ?? 0,
    facts.trade_good_worth,
    totals(facts, draft).cost,
  );
  return pick ? { ...draft, ...pick } : draft;
}

// The ids of the staged values, as the sheet sends them back in `setCount`.
export const buildKey = (unit: number, place: string) => `build:${unit}:${place}`;

/** The staged purchases of a unit at one of its places. */
export const purchases = (draft: ProductionDraft, unit: number, place: string) =>
  draft.builds.filter((build) => build.unit === unit && build.place === place).length;

/**
 * A `setCount` of the build sheet: "build:<unit>:<place>" with the purchases that are wanted
 * there. One more is added at the end; one less takes the last one back. Any other key leaves
 * the draft as it is.
 */
export function setCount(
  facts: ProductionFacts,
  draft: ProductionDraft,
  key: string,
  value: number,
): ProductionDraft {
  const [kind, index, ...rest] = key.split(":");
  if (kind !== "build" || rest.length === 0) {
    return draft;
  }
  const unit = Number(index);
  const place = rest.join(":");
  let now = draft;
  for (let have = purchases(now, unit, place); have < value; have++) {
    const next = addBuild(facts, now, unit, place);
    if (next === now) {
      break;
    }
    now = next;
  }
  for (let have = purchases(now, unit, place); have > Math.max(0, value); have--) {
    const last = now.builds.findLastIndex((build) => build.unit === unit && build.place === place);
    now = removeBuild(now, last);
  }
  return now;
}

/**
 * The steps of the staged production: each purchase with its place, when the game asks for one,
 * then what pays, then the end. The game asks for the payments between the purchases; the plan
 * gives them in this order whenever it is asked (`plan_answer` in `ti4-host`).
 */
export function stepsOf(facts: ProductionFacts, draft: ProductionDraft): ProductionStep[] {
  const steps: ProductionStep[] = [];
  for (const build of draft.builds) {
    const unit = facts.units[build.unit];
    if (!unit) {
      continue;
    }
    steps.push({ kind: "produce", unit: unit.unit });
    if (unit.places.length > 1) {
      steps.push({ kind: "place", destination: build.place });
    }
  }
  for (const planet of draft.planets) {
    const face = facts.planets.find((item) => item.planet === planet)?.face;
    steps.push({ kind: "pay", source: exhaustId(planet, face) });
  }
  for (let good = 0; good < draft.goods; good++) {
    steps.push({ kind: "pay", source: GOODS });
  }
  for (let commodity = 0; commodity < (draft.commodities ?? 0); commodity++) {
    steps.push({ kind: "pay", source: COMMODITY });
  }
  steps.push({ kind: "done_producing" });
  return steps;
}

/**
 * The production of some steps, staged again on the facts of now: a plan that stopped, an undo,
 * a reload. A step that no longer fits is left out.
 */
export function draftOf(facts: ProductionFacts, steps: PlanStep[]): ProductionDraft {
  let draft = EMPTY;
  // A payment alone is not of a production: other decisions are paid with planets too.
  if (!steps.some((step) => step.kind === "produce")) {
    return draft;
  }
  for (const step of steps) {
    if (step.kind === "produce") {
      const unit = facts.units.findIndex((item) => item.unit === step.unit);
      draft = unit < 0 ? draft : addBuild(facts, draft, unit, facts.units[unit].places[0]);
    } else if (step.kind === "place") {
      const last = draft.builds.at(-1);
      if (last && facts.units[last.unit].places.includes(step.destination)) {
        draft = {
          ...draft,
          builds: [...draft.builds.slice(0, -1), { ...last, place: step.destination }],
        };
      }
    } else if (step.kind === "pay") {
      // A planet is named by the option of the game that exhausts it, with its face.
      const planet = facts.planets.find(
        (item) => exhaustId(item.planet, item.face) === step.source,
      )?.planet;
      draft =
        step.source === GOODS
          ? setGoods(facts, draft, draft.goods + 1)
          : step.source === COMMODITY
            ? setGoods(facts, draft, (draft.commodities ?? 0) + 1, COMMODITY)
            : planet === undefined || draft.planets.includes(planet)
              ? draft
              : togglePlanet(facts, draft, planet);
    }
  }
  return draft;
}
