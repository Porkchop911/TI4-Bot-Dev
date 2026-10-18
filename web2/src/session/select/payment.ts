// A payment on the map, staged with the question that opens the bill: the ready planets that the
// engine lists with their worth, and trade goods on a counter. The game asks the payment one
// planet or trade good at a time; the staged payment is sent as `pay` steps of a plan, which the
// game takes as it asks (`plan_answer` in `ti4-host/src/planning/steps.rs`). Leadership's
// purchase of command tokens and a paid research use it.
import type { ActionButtonView, BoardTaskView, PaymentView } from "../../model";
import { plural } from "../../model";
import type { PlanStep } from "../movementPlan";
import type { PaymentHints, PaymentPlanet, SessionUpdate } from "../wire";

/** The id of the trade goods of the payment (the option of the game). */
export const GOODS = "trade_good";
/** The id of a commodity spent as a trade good (the option of the game, the Keleres agent). */
export const COMMODITY = "commodity";

/** What can pay, as the engine lists it in the details of the question (`PaymentHints`). */
export interface PayOffer {
  planets: PaymentPlanet[];
  goods: number;
  goodWorth: number;
  /** Commodities that pay as trade goods (the Keleres agent). */
  commodities: number;
}

/** What the player staged to pay with. */
export interface Paying {
  /** The planets that pay, in the order that they were chosen. */
  planets: string[];
  goods: number;
  commodities?: number;
}

export const NO_PAYMENT: Paying = { planets: [], goods: 0, commodities: 0 };

/** The payment option of the game that exhausts a planet: `exhaust|<planet>`, with its face. */
export const exhaustId = (planet: string, face?: string | null) =>
  face ? `exhaust|${planet}|${face}` : `exhaust|${planet}`;

/** The `faces` of a task on the map: the planets that pay with their other value. */
export function facesOf(planets: { planet: string; face?: string | null }[]): {
  faces?: Record<string, string>;
} {
  const faces = planets.flatMap(({ planet, face }) => (face ? [[planet, face] as const] : []));
  return faces.length ? { faces: Object.fromEntries(faces) } : {};
}

/** "Jord 4", and "Jord 3 influence" for a planet that pays with its other value. */
export const planetPays = (name: string, worth: number, face?: string | null) =>
  face ? `${name} ${worth} ${face}` : `${name} ${worth}`;

/**
 * Auto-pay over planets, trade goods and commodities: the payment of `cost` with the least
 * waste, then with the fewest trade goods. Commodities are spent before trade goods. Null when
 * nothing can pay it.
 */
export function leastWaste(
  planets: { id: string; worth: number }[],
  goods: number,
  commodities: number,
  goodWorth: number,
  cost: number,
): Paying | null {
  const top = cost + Math.max(0, ...planets.map((planet) => planet.worth));
  const best: (string[] | null)[] = Array.from({ length: top + 1 }, () => null);
  best[0] = [];
  for (const planet of planets) {
    for (let total = top; total >= planet.worth; total--) {
      const before = best[total - planet.worth];
      const now = best[total];
      if (before && total - planet.worth < cost && (!now || now.length > before.length + 1)) {
        best[total] = [...before, planet.id];
      }
    }
  }
  const worth = Math.max(1, goodWorth);
  let pick: { planets: string[]; tokens: number; waste: number } | null = null;
  for (let total = 0; total <= top; total++) {
    const chosen = best[total];
    const rest = Math.max(0, cost - total);
    if (!chosen || rest > (goods + commodities) * goodWorth) {
      continue;
    }
    const tokens = Math.ceil(rest / worth);
    const waste = total + tokens * goodWorth - cost;
    // Less waste first, then fewer tokens: the planets are readied again, the goods are gone.
    if (!pick || waste < pick.waste || (waste === pick.waste && tokens < pick.tokens)) {
      pick = { planets: chosen, tokens, waste };
    }
  }
  if (!pick) {
    return null;
  }
  const spent = Math.min(commodities, pick.tokens);
  return { planets: pick.planets, goods: pick.tokens - spent, commodities: spent };
}

/** The planets, trade goods and commodities of the look-ahead of a payment of the engine. */
export const payOfferOf = (hints: PaymentHints): PayOffer => ({
  planets: hints.planets,
  goods: hints.trade_goods,
  goodWorth: hints.trade_good_worth,
  commodities: hints.commodities,
});

/** What the staged payment is worth. */
export const paidBy = (offer: PayOffer, paying: Paying) =>
  paying.planets.reduce(
    (total, id) => total + (offer.planets.find((planet) => planet.id === id)?.worth ?? 0),
    0,
  ) +
  (paying.goods + (paying.commodities ?? 0)) * offer.goodWorth;

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
    ...paying.planets.map((id) => {
      const planet = offer.planets.find((item) => item.id === id);
      return planetPays(names.get(id) ?? id, planet?.worth ?? 0, planet?.face);
    }),
    ...(paying.goods ? [plural(paying.goods, "trade good")] : []),
    ...(paying.commodities ? [plural(paying.commodities, "commodity", "commodities")] : []),
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
    commodities: offer.commodities
      ? { id: COMMODITY, value: paying.commodities ?? 0, max: offer.commodities }
      : null,
    actions: [
      action("Auto-pay", { type: "payment", action: "auto" }, false),
      action(
        "Clear payment",
        { type: "payment", action: "reset" },
        !paying.planets.length && !paying.goods && !paying.commodities,
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
      ...facesOf(offer.planets.map(({ id, face }) => ({ planet: id, face }))),
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

/** The trade goods of the payment, or its commodities (`source` is `COMMODITY`). */
export const goodsPaying = (
  offer: PayOffer,
  paying: Paying,
  value: number,
  source: string = GOODS,
): Paying =>
  source === COMMODITY
    ? { ...paying, commodities: Math.max(0, Math.min(value, offer.commodities)) }
    : { ...paying, goods: Math.max(0, Math.min(value, offer.goods)) };

/**
 * Auto-pay: the planets, trade goods and commodities that pay `cost` with the least waste, then
 * with the fewest trade goods. Null when nothing can pay it.
 */
export const autoPaying = (offer: PayOffer, cost: number): Paying | null =>
  leastWaste(offer.planets, offer.goods, offer.commodities, offer.goodWorth, cost);

/**
 * The `pay` steps of a staged payment: the planets in the order chosen, each with its face, then
 * the trade goods, then the commodities.
 */
export const paySteps = (offer: PayOffer, paying: Paying): PlanStep[] => [
  ...paying.planets.map((planet): PlanStep => ({
    kind: "pay",
    source: exhaustId(planet, offer.planets.find((item) => item.id === planet)?.face),
  })),
  ...Array.from({ length: paying.goods }, (): PlanStep => ({ kind: "pay", source: GOODS })),
  ...Array.from({ length: paying.commodities ?? 0 }, (): PlanStep => ({
    kind: "pay",
    source: COMMODITY,
  })),
];
