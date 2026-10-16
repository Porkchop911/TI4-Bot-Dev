// A payment on the map, staged with the question that opens the bill: the ready planets that the
// engine lists with their worth, and trade goods on a counter. The game asks the payment one
// planet or trade good at a time; the staged payment is sent as `pay` steps of a plan, which the
// game takes as it asks (`plan_answer` in `ti4-host/src/planning/steps.rs`). Leadership's
// purchase of command tokens and a paid research use it.
import type { ActionButtonView, BoardTaskView, PaymentView } from "../../model";
import { plural } from "../../model";
import type { PlanStep } from "../movementPlan";
import type { SessionUpdate } from "../wire";

/** The id of the trade goods of the payment (the option of the game). */
export const GOODS = "trade_good";

/** What can pay, as the engine lists it in the details of the question. */
export interface PayOffer {
  planets: { id: string; worth: number }[];
  goods: number;
  goodWorth: number;
}

/** What the player staged to pay with. */
export interface Paying {
  /** The planets that pay, in the order that they were chosen. */
  planets: string[];
  goods: number;
}

export const NO_PAYMENT: Paying = { planets: [], goods: 0 };

const num = (value: unknown): number | null => (typeof value === "number" ? value : null);

/** The planets and trade goods of a payment of the engine (`planets`, `trade_goods`, `trade_good_worth`). */
export function payOfferOf(value: unknown): PayOffer | null {
  const offer = value as Record<string, unknown> | null;
  if (!offer || !Array.isArray(offer.planets)) {
    return null;
  }
  return {
    planets: (offer.planets as { id?: unknown; worth?: unknown }[]).flatMap((planet) =>
      typeof planet.id === "string" && typeof planet.worth === "number"
        ? [{ id: planet.id, worth: planet.worth }]
        : [],
    ),
    goods: num(offer.trade_goods) ?? 0,
    goodWorth: num(offer.trade_good_worth) ?? 1,
  };
}

/** What the staged payment is worth. */
export const paidBy = (offer: PayOffer, paying: Paying) =>
  paying.planets.reduce(
    (total, id) => total + (offer.planets.find((planet) => planet.id === id)?.worth ?? 0),
    0,
  ) +
  paying.goods * offer.goodWorth;

/** The planets of the map by id, with their names. */
export const planetNames = (update: SessionUpdate) =>
  new Map(
    (update.view.board.map_tiles ?? []).flatMap((tile) =>
      (tile.planets ?? []).map((planet) => [planet.id, planet.label] as const),
    ),
  );

/**
 * The open payment: the payment bar and the payment line of the footer, and the map, where the
 * planets are chosen. `unit` is "resources" or "influence".
 */
export function paymentOf(
  update: SessionUpdate,
  offer: PayOffer,
  paying: Paying,
  cost: number,
  unit: string,
): { payment: PaymentView; task: BoardTaskView } {
  const names = planetNames(update);
  const paid = paidBy(offer, paying);
  const summary = [
    ...paying.planets.map(
      (id) =>
        `${names.get(id) ?? id} ${offer.planets.find((planet) => planet.id === id)?.worth ?? 0}`,
    ),
    ...(paying.goods ? [plural(paying.goods, "trade good")] : []),
  ].join(" + ");
  const short = cost - (paid - paying.goods * offer.goodWorth);
  const worth = Math.max(1, offer.goodWorth);
  const rest = short >= 0 && short % worth === 0 ? short / worth : null;
  const action = (
    label: string,
    intent: ActionButtonView["intent"],
    disabled: boolean,
  ): ActionButtonView => ({ label, tone: "quiet", intent, disabled });
  const payment: PaymentView = {
    title: "Payment",
    cost,
    paid,
    unit,
    editable: true,
    summary,
    goods: offer.goods
      ? {
          id: GOODS,
          value: paying.goods,
          max: offer.goods,
          rest: rest !== null && rest !== paying.goods && rest <= offer.goods ? rest : null,
        }
      : null,
    actions: [
      action("Auto-pay", { type: "payment", action: "auto" }, false),
      action(
        "Clear payment",
        { type: "payment", action: "reset" },
        !paying.planets.length && !paying.goods,
      ),
    ],
  };
  return {
    payment,
    task: {
      target: "planet",
      kind: "pay",
      interactive: true,
      values: Object.fromEntries(offer.planets.map((planet) => [planet.id, planet.worth])),
      chosen: Object.fromEntries(paying.planets.map((id) => [id, true])),
      verb: "Exhaust",
      unit,
      payment,
    },
  };
}

/** A planet of the payment is chosen, or given back. Null for a planet that does not pay. */
export function togglePaying(offer: PayOffer, paying: Paying, planet: string): Paying | null {
  if (!offer.planets.some((item) => item.id === planet)) {
    return null;
  }
  return {
    ...paying,
    planets: paying.planets.includes(planet)
      ? paying.planets.filter((id) => id !== planet)
      : [...paying.planets, planet],
  };
}

/** The trade goods of the payment. */
export const goodsPaying = (offer: PayOffer, paying: Paying, value: number): Paying => ({
  ...paying,
  goods: Math.max(0, Math.min(value, offer.goods)),
});

/**
 * Auto-pay: the planets and trade goods that pay `cost` with the least waste, then with the
 * fewest trade goods. Null when nothing can pay it.
 */
export function autoPaying(offer: PayOffer, cost: number): Paying | null {
  const top = cost + Math.max(0, ...offer.planets.map((planet) => planet.worth));
  const best: (string[] | null)[] = Array.from({ length: top + 1 }, () => null);
  best[0] = [];
  for (const planet of offer.planets) {
    for (let total = top; total >= planet.worth; total--) {
      const before = best[total - planet.worth];
      const now = best[total];
      if (before && total - planet.worth < cost && (!now || now.length > before.length + 1)) {
        best[total] = [...before, planet.id];
      }
    }
  }
  let pick: { planets: string[]; goods: number; waste: number } | null = null;
  for (let total = 0; total <= top; total++) {
    const chosen = best[total];
    const rest = Math.max(0, cost - total);
    if (!chosen || rest > offer.goods * offer.goodWorth) {
      continue;
    }
    const goods = Math.ceil(rest / Math.max(1, offer.goodWorth));
    const waste = total + goods * offer.goodWorth - cost;
    if (!pick || waste < pick.waste || (waste === pick.waste && goods < pick.goods)) {
      pick = { planets: chosen, goods, waste };
    }
  }
  return pick ? { planets: pick.planets, goods: pick.goods } : null;
}

/** The `pay` steps of a staged payment: the planets in the order chosen, then the trade goods. */
export const paySteps = (paying: Paying): PlanStep[] => [
  ...paying.planets.map((planet): PlanStep => ({ kind: "pay", source: `exhaust|${planet}` })),
  ...Array.from({ length: paying.goods }, (): PlanStep => ({ kind: "pay", source: GOODS })),
];
