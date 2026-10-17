// The staged production of one use of PRODUCTION: the purchases, in the order that the player
// made them, and what pays for them. Nothing of it is sent before "Produce units". The prices,
// the places and the limits are facts of the engine; this file only adds up what is staged.
import type { PlanStep, ProductionStep } from "./movementPlan";
import type { ProductionFacts } from "./wire";

export interface ProductionDraft {
  /** One purchase each: a unit of the facts, and the place of what it makes. */
  builds: { unit: number; place: string }[];
  /** The planets that are exhausted for the payment, in the order that they were chosen. */
  planets: string[];
  /** The trade goods that are spent. */
  goods: number;
}

export const EMPTY: ProductionDraft = { builds: [], planets: [], goods: 0 };

export const isEmpty = (draft: ProductionDraft) =>
  draft.builds.length === 0 && draft.planets.length === 0 && draft.goods === 0;

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
  /** What the ready planets and the trade goods of the seat can pay. */
  available: number;
  /** The ships that count against the fleet pool, with the staged ones. */
  fleet: number;
  /** What uses capacity in the space area, with the staged units there. */
  cargo: number;
  /** The capacity of the ships in the space area, with the staged ships. */
  transport: number;
  /** The staged units of each unit of the facts. */
  made: number[];
}

export function totals(facts: ProductionFacts, draft: ProductionDraft): Totals {
  const size = sizes(facts, draft);
  const made = facts.units.map(() => 0);
  let printed = 0;
  let fleet = facts.fleet_supply.charged;
  let cargo = facts.transport.used;
  let transport = facts.transport.limit;
  draft.builds.forEach((build, index) => {
    const unit = facts.units[build.unit];
    if (!unit) {
      return;
    }
    made[build.unit] += size[index];
    printed += unit.cost;
    fleet += unit.fleet ? size[index] : 0;
    cargo += unit.cargo && build.place === "space" ? size[index] : 0;
    transport += build.place === "space" ? unit.capacity * size[index] : 0;
  });
  const worth = new Map(facts.planets.map((planet) => [planet.planet, planet.worth]));
  return {
    units: size.reduce((sum, count) => sum + count, 0),
    capacity: room(facts),
    cost: facts.free ? 0 : Math.max(0, Math.max(0, printed - facts.discount) - facts.credit),
    paid:
      draft.planets.reduce((sum, planet) => sum + (worth.get(planet) ?? 0), 0) +
      draft.goods * facts.trade_good_worth,
    available:
      facts.planets.reduce((sum, planet) => sum + planet.worth, 0) +
      facts.trade_goods * facts.trade_good_worth,
    fleet,
    cargo,
    transport,
    made,
  };
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
  not_offered: "Cannot be produced here",
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

export const setGoods = (
  facts: ProductionFacts,
  draft: ProductionDraft,
  goods: number,
): ProductionDraft => ({ ...draft, goods: Math.max(0, Math.min(goods, facts.trade_goods)) });

export const clearPayment = (draft: ProductionDraft): ProductionDraft => ({
  ...draft,
  planets: [],
  goods: 0,
});

/**
 * The payment with the least waste: planets before trade goods, and of two payments with the
 * same waste the one with fewer planets.
 */
export function autoPay(facts: ProductionFacts, draft: ProductionDraft): ProductionDraft {
  const { cost } = totals(facts, draft);
  const planets = facts.planets;
  const goods = facts.trade_goods * facts.trade_good_worth;
  // The best set of planets for each sum that planets can pay, up to the cost and one planet over.
  const top = cost + Math.max(0, ...planets.map((planet) => planet.worth));
  const best: (string[] | null)[] = Array.from({ length: top + 1 }, () => null);
  best[0] = [];
  for (const planet of planets) {
    for (let sum = top; sum >= planet.worth; sum--) {
      const before = best[sum - planet.worth];
      const now = best[sum];
      if (before && sum - planet.worth < cost && (!now || now.length > before.length + 1)) {
        best[sum] = [...before, planet.planet];
      }
    }
  }
  let pick: { planets: string[]; goods: number; waste: number } | null = null;
  for (let sum = 0; sum <= top; sum++) {
    const chosen = best[sum];
    const rest = Math.max(0, cost - sum);
    if (!chosen || rest > goods) {
      continue;
    }
    const spent = Math.ceil(rest / Math.max(1, facts.trade_good_worth));
    const waste = sum + spent * facts.trade_good_worth - cost;
    // Less waste first, then fewer trade goods: the planets are readied again, the goods are gone.
    if (!pick || waste < pick.waste || (waste === pick.waste && spent < pick.goods)) {
      pick = { planets: chosen, goods: spent, waste };
    }
  }
  return pick ? { ...draft, planets: pick.planets, goods: pick.goods } : draft;
}

// The ids of the staged values, as the sheet sends them back in `setCount`.
export const GOODS = "trade_good";
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
    steps.push({ kind: "pay", source: `exhaust|${planet}` });
  }
  for (let good = 0; good < draft.goods; good++) {
    steps.push({ kind: "pay", source: GOODS });
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
      draft =
        step.source === GOODS
          ? setGoods(facts, draft, draft.goods + 1)
          : draft.planets.includes(step.source.slice(8))
            ? draft
            : togglePlanet(facts, draft, step.source.slice(8));
    }
  }
  return draft;
}
