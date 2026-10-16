// A gain of command tokens, with the purchase of more tokens for influence when the game offers
// one (Leadership: `details.purchase`). The game asks one token at a time and one payment at a
// time; here the whole of it is one screen: the bought tokens on a counter, the pools of the
// command sheet with what is gained and bought, and the payment on the map. It is sent as one
// plan (`pool`, `purchase` and `pay` steps), which the game answers in the order that it asks.
import type { BlockView, BoardTaskView, PaymentView } from "../../model";
import { plural } from "../../model";
import type { PlanStep } from "../movementPlan";
import type { Choice, SessionUpdate } from "../wire";
import {
  type PayOffer,
  autoPaying,
  goodsPaying,
  paidBy,
  payOfferOf,
  paySteps,
  paymentOf,
  togglePaying,
} from "./payment";
import { type Pools, placeProblem, poolChange, poolRows, poolsOf } from "./tokens";
import { type Editor, num } from "./editor";

/** What the player staged: tokens bought, and what pays for them. */
export interface PurchaseDraft {
  buy: number;
  /** The pools after the gain and the purchase. Null: the pools of now. */
  pools: Pools | null;
  /** The planets that pay, in the order that they were chosen. */
  planets: string[];
  goods: number;
  commodities?: number;
}

export const NO_PURCHASE: PurchaseDraft = {
  buy: 0,
  pools: null,
  planets: [],
  goods: 0,
  commodities: 0,
};

interface Offer extends PayOffer {
  cost: number;
  max: number;
}

interface Question {
  mode: "gain" | "buy";
  now: Pools;
  /** The tokens that the game still gains, before any purchase. */
  gain: number;
  /** The purchase, when the seat can buy tokens now. */
  offer: Offer | null;
}

/** The pools, with the options of a gain into each. */
const POOLS: [keyof Pools, string][] = [
  ["tactic", "tactic_tokens"],
  ["fleet", "fleet_tokens"],
  ["strategic", "strategic_tokens"],
];

function offerOf(value: unknown): Offer | null {
  const offer = value as Record<string, unknown> | null;
  const cost = num(offer?.cost);
  const max = num(offer?.max);
  const paying = payOfferOf(value);
  return paying && cost !== null && max !== null ? { ...paying, cost, max } : null;
}

/**
 * A question that gains command tokens into a pool, or that offers a purchase for influence.
 * Null for any other question.
 */
function questionOf(choice: Choice): Question | null {
  const details = choice.details ?? {};
  const now = poolsOf(details.pools);
  // The window of Leadership's secondary is the purchase question itself, with the kind of
  // the window.
  if (!now || (details.kind !== "command_tokens" && details.kind !== "strategy_secondary")) {
    return null;
  }
  const ids = new Set(choice.options.map((option) => option.id));
  if (
    details.kind === "command_tokens" &&
    details.mode === "gain" &&
    POOLS.every(([, id]) => ids.has(id)) &&
    choice.options.every((option) => option.kind === "pool")
  ) {
    return {
      mode: "gain",
      now,
      gain: Math.max(1, num(details.tokens_to_place) ?? 1),
      offer: offerOf(details.purchase),
    };
  }
  const offer = offerOf(details.purchase);
  if (details.mode === "buy" && offer && ids.has("yes") && ids.has("no") && ids.size === 2) {
    return { mode: "buy", now, gain: 0, offer };
  }
  return null;
}

const sum = (pools: Pools) => pools.tactic + pools.fleet + pools.strategic;

/** What the staged payment is worth, in influence. */
const paidOf = (offer: Offer | null, draft: PurchaseDraft) => (offer ? paidBy(offer, draft) : 0);

export interface PurchaseView {
  blocks: BlockView[];
  /** The map is the payment while tokens are bought. */
  task: BoardTaskView | null;
  payment: PaymentView | null;
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  /** The main button. */
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
}

/** The gain and the purchase of command tokens. Null for any other question. */
export function selectPurchase(
  update: SessionUpdate,
  choice: Choice,
  draft: PurchaseDraft | null,
): PurchaseView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_PURCHASE;
  const { now, gain, offer } = question;
  const buy = offer ? Math.min(staging.buy, offer.max) : 0;
  const total = sum(now) + gain + buy;
  const after = staging.pools ?? now;
  const placed = sum(after);
  const left = total - placed;
  const cost = offer ? buy * offer.cost : 0;
  const paid = paidOf(offer, staging);

  const rows = poolRows(now, after, now, left);
  const blocks: BlockView[] = [
    ...(offer
      ? [
          {
            kind: "card" as const,
            title: "Buy command tokens",
            aside: `${buy} bought · ${cost} influence`,
            rows: [
              {
                icon: "token" as const,
                title: ["Command tokens"],
                subtitle: `${offer.cost} influence each`,
                control: {
                  kind: "counter" as const,
                  counter: { id: "buy", value: buy, max: offer.max, label: "bought token" },
                },
              },
            ],
          },
        ]
      : []),
    {
      kind: "card",
      title: "Command pools",
      aside: [
        ...(gain ? [`${gain} gained`] : []),
        ...(offer ? [`${buy} bought`] : []),
        `${placed - sum(now)} / ${total - sum(now)} placed`,
      ].join(" · "),
      bad: left !== 0,
      rows,
    },
  ];

  const open = offer && buy ? paymentOf(update, offer, staging, cost, "influence") : null;
  const payment: PaymentView | null = open?.payment ?? null;
  const task: BoardTaskView | null = open?.task ?? null;
  const placing = placeProblem(left);
  const problem = placing
    ? `${placing}.`
    : paid < cost
      ? `Pay ${cost - paid} more influence.`
      : null;
  const change = poolChange(now, after);
  return {
    blocks,
    task,
    payment,
    note: problem ?? change,
    problem,
    label:
      question.mode === "buy" && !buy
        ? "Buy nothing"
        : buy
          ? `Buy ${plural(buy, "token")}`
          : "Send",
    changed:
      !!draft &&
      (staging.buy > 0 ||
        staging.pools !== null ||
        staging.planets.length > 0 ||
        staging.goods > 0 ||
        (staging.commodities ?? 0) > 0),
  };
}

/** A counter of the screen: `buy`, or a pool (`pools.t`, `pools.f`, `pools.s`). */
export function countPurchase(
  choice: Choice,
  draft: PurchaseDraft | null,
  key: string,
  value: number,
): PurchaseDraft | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_PURCHASE;
  if (key === "buy" && question.offer) {
    const buy = Math.max(0, Math.min(value, question.offer.max));
    // Fewer bought tokens: the pools that held more than there are tokens start again.
    const pools =
      staging.pools && sum(staging.pools) > sum(question.now) + question.gain + buy
        ? null
        : staging.pools;
    return {
      ...staging,
      buy,
      pools,
      ...(buy ? {} : { planets: [], goods: 0, commodities: 0 }),
    };
  }
  const pool = POOLS.find(([name]) => `pools.${name[0]}` === key)?.[0];
  if (!pool) {
    return null;
  }
  const pools = { ...(staging.pools ?? question.now), [pool]: Math.max(question.now[pool], value) };
  return { ...staging, pools };
}

/** A planet of the payment is chosen, or given back. */
export function togglePurchasePlanet(
  choice: Choice,
  draft: PurchaseDraft | null,
  planet: string,
): PurchaseDraft | null {
  const offer = questionOf(choice)?.offer;
  const staging = draft ?? NO_PURCHASE;
  const paying = offer && staging.buy ? togglePaying(offer, staging, planet) : null;
  return paying ? { ...staging, ...paying } : null;
}

/** The trade goods of the payment, or its commodities. */
export function setPurchaseGoods(
  choice: Choice,
  draft: PurchaseDraft | null,
  value: number,
  source?: string,
): PurchaseDraft | null {
  const offer = questionOf(choice)?.offer;
  const staging = draft ?? NO_PURCHASE;
  return offer && staging.buy
    ? { ...staging, ...goodsPaying(offer, staging, value, source) }
    : null;
}

/**
 * Auto-pay: the planets and trade goods that pay the bought tokens with the least waste, then
 * with the fewest trade goods. "Clear payment" is `reset`.
 */
export function payPurchase(
  choice: Choice,
  draft: PurchaseDraft | null,
  how: "auto" | "reset",
): PurchaseDraft | null {
  const offer = questionOf(choice)?.offer;
  const staging = draft ?? NO_PURCHASE;
  if (!offer || !staging.buy) {
    return null;
  }
  if (how === "reset") {
    return { ...staging, planets: [], goods: 0, commodities: 0 };
  }
  const pick = autoPaying(offer, staging.buy * offer.cost);
  return pick ? { ...staging, ...pick } : staging;
}

/**
 * The plan that answers the gain and the purchase: a pool for each gained token, then for each
 * bought token "yes" and its pool, then "no", and the payment, which the game takes as it asks
 * for it. Null while the screen cannot be sent.
 */
export function purchasePlan(choice: Choice, draft: PurchaseDraft | null): PlanStep[] | null {
  const question = questionOf(choice);
  const staging = draft ?? NO_PURCHASE;
  if (!question) {
    return null;
  }
  const { now, gain, offer } = question;
  const buy = offer ? Math.min(staging.buy, offer.max) : 0;
  const after = staging.pools ?? now;
  if (sum(after) !== sum(now) + gain + buy || paidOf(offer, staging) < (offer?.cost ?? 0) * buy) {
    return null;
  }
  // The added tokens, pool by pool: the gained ones first, then one for each purchase.
  const added = POOLS.flatMap(([key, id]) =>
    Array.from({ length: after[key] - now[key] }, () => id),
  );
  const steps: PlanStep[] = [
    ...added.slice(0, gain).map((pool): PlanStep => ({ kind: "pool", pool })),
    ...added.slice(gain).flatMap((pool): PlanStep[] => [
      { kind: "purchase", buy: true },
      { kind: "pool", pool },
    ]),
    ...(offer ? [{ kind: "purchase", buy: false } as PlanStep] : []),
    ...(offer ? paySteps(offer, staging) : []),
  ];
  return steps;
}

/** The gain and the purchase of command tokens as an editor (`editors.ts`): sent as one plan. */
export const purchaseEditor: Editor<PurchaseDraft> = {
  view: (update, choice, draft) => {
    const view = selectPurchase(update, choice, draft);
    const plan = view && purchasePlan(choice, draft);
    return view && { ...view, send: plan && { choices: [], plan } };
  },
  count: countPurchase,
  planet: togglePurchasePlanet,
  goods: setPurchaseGoods,
  pay: payPurchase,
};
