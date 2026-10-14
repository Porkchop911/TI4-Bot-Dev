import type { PendingChoiceDto } from "../protocol/types.ts";
import {
  buildPaymentSteps,
  derivePaymentOffer,
  paymentOptionForPlanet,
  type PaymentDraft,
  type PaymentOffer,
  type PaymentStep,
} from "./paymentDraft.ts";
import { optionUnit, productionSystem } from "./productionDraft.ts";

/**
 * One payment for a whole production. The engine asks to pay build by build (`pay_resources`, one
 * question per build the credit does not cover), so the client asks the player ONCE, at the first
 * question, for everything the staged builds cost, and then answers the engine's later payment
 * questions from that single plan. Nothing here changes what the engine asks or accepts: every
 * answer is checked against the question that is actually open.
 */
export type ProductionPayMode =
  /** Waiting for the player to confirm the one payment for all builds. */
  | "ask"
  /** The plan is confirmed; the shell answers the engine's payment questions from `remaining`. */
  | "auto"
  /** Asking build by build (the plan stopped or the player chose it), suggestion prefilled. */
  | "manual";

export interface ProductionPay {
  actor: string;
  system: string;
  /** What each staged build costs, in the order it is sent (printed cost, 0 when free). */
  costs: number[];
  /** How many builds have been sent to the engine so far. */
  submitted: number;
  mode: ProductionPayMode;
  /** The part of the confirmed plan not yet spent. */
  remaining: PaymentDraft;
  /** The payment question the player's panel took; it stays on screen until the engine moves on. */
  panelNonce?: string;
  /** The payment question the panel or the auto-answer already took; it is not answered twice. */
  handledNonce?: string;
  /** Why the plan stopped, shown beside the payment that is asked by hand. */
  note?: string;
}

const isBuild = (option: { id: string; kind?: string }) =>
  option.id !== "decline" && option.kind !== "decline";

/** What each unit costs at the production question; null when a unit is not uniquely offered. */
export function buildCosts(choice: PendingChoiceDto, units: readonly string[]): number[] | null {
  const options = choice.options.filter(isBuild);
  const costs: number[] = [];
  for (const unit of units) {
    const matching = options.filter((option) => optionUnit(option) === unit);
    if (matching.length !== 1) return null;
    const payload = matching[0].payload ?? {};
    if (payload.free_this_use === true || payload.exchange === true) {
      costs.push(0);
      continue;
    }
    const value = Number.isSafeInteger(payload.printed_cost) ? payload.printed_cost : payload.cost;
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) return null;
    costs.push(value);
  }
  return costs;
}

/** The plan for staged builds; null for a single build (its one payment question is the plan). */
export function newProductionPay(
  choice: PendingChoiceDto,
  units: readonly string[],
): ProductionPay | null {
  const costs = buildCosts(choice, units);
  if (!costs || units.length < 2) return null;
  return {
    actor: choice.actor,
    system: productionSystem(choice),
    costs,
    submitted: 0,
    mode: "ask",
    remaining: { planetIds: [], tradeGoods: 0 },
  };
}

/** Builds (and their cost) still to be sent after the ones already sent. */
export function unsentBuilds(plan: ProductionPay): { count: number; cost: number } {
  const rest = plan.costs.slice(plan.submitted);
  return { count: rest.length, cost: rest.reduce((sum, cost) => sum + cost, 0) };
}

/** True for the engine's resource payment question of this production. */
export function isProductionPayQuestion(
  choice: PendingChoiceDto | null | undefined,
  plan: ProductionPay | null | undefined,
): boolean {
  if (!choice || !plan) return false;
  if (choice.actor !== plan.actor || choice.context?.subtype !== "pay_resources") return false;
  const target = choice.context.target;
  if (target && "System" in target && String(target.System) !== plan.system) return false;
  return derivePaymentOffer(choice).currency === "Resources";
}

export interface PlanPanel {
  /** The whole production's bill, as the one panel's `owed`. */
  offer: PaymentOffer;
  /** Builds the panel pays for: the open one and the unsent ones. */
  builds: number;
  total: number;
}

/**
 * The panel for the first payment question: the open question's bill plus every unsent build,
 * over the planets and trade goods the open question offers. Null when no build follows this one
 * (a plain single payment) or when this is not the plan's question.
 */
export function planPanel(
  choice: PendingChoiceDto,
  base: PaymentOffer,
  plan: ProductionPay | null | undefined,
): PlanPanel | null {
  if (!plan || plan.mode !== "ask" || !isProductionPayQuestion(choice, plan)) return null;
  const unsent = unsentBuilds(plan);
  if (unsent.count === 0) return null;
  const total = base.owed + unsent.cost;
  return {
    offer: { ...base, owed: total, totalAmount: base.alreadyPaid + total },
    builds: unsent.count + 1,
    total,
  };
}

export interface QuestionAllocation {
  /** What this question is paid with. */
  draft: PaymentDraft;
  /** The plan after this question. */
  rest: PaymentDraft;
}

/**
 * Pays the open question from the plan: planets in plan order until the bill is covered, then trade
 * goods for what planets leave. Overpayment is fine (the engine carries it as credit for the next
 * build), so a plan that covers the total covers every question in order. Null when the plan cannot
 * pay this question as offered (a planet is gone, or trade goods fall short): the caller asks by
 * hand.
 */
export function allocateQuestion(
  offer: PaymentOffer,
  plan: PaymentDraft,
  tradeGoodsAvailable: number,
): QuestionAllocation | null {
  if (offer.owed <= 0) return null;
  const planetIds: string[] = [];
  const spent = new Set<string>();
  let sum = 0;
  for (const staged of plan.planetIds) {
    if (sum >= offer.owed) break;
    const key = staged.replace(/^exhaust\|/, "").split("|")[0];
    const option =
      offer.planets.find((planet) => planet.id === staged) ?? paymentOptionForPlanet(offer, key);
    if (!option || option.worth <= 0) return null;
    planetIds.push(option.id);
    spent.add(staged);
    sum += option.worth;
  }
  let tradeGoods = 0;
  if (sum < offer.owed) {
    tradeGoods = Math.ceil((offer.owed - sum) / Math.max(1, offer.tradeGoodWorth));
    if (!offer.hasTradeGoodOption || tradeGoods > plan.tradeGoods || tradeGoods > tradeGoodsAvailable)
      return null;
  }
  return {
    draft: { planetIds, tradeGoods },
    rest: {
      planetIds: plan.planetIds.filter((id) => !spent.has(id)),
      tradeGoods: plan.tradeGoods - tradeGoods,
    },
  };
}

/** The payment batch steps that answer the open question from the plan, or null when it cannot. */
export function autoPaymentFor(
  choice: PendingChoiceDto,
  plan: PaymentDraft,
  tradeGoodsAvailable: number,
): { steps: PaymentStep[]; rest: PaymentDraft; summary: string } | null {
  const offer = derivePaymentOffer(choice);
  const allocation = allocateQuestion(offer, plan, tradeGoodsAvailable);
  if (!allocation) return null;
  const steps = buildPaymentSteps(choice, offer, allocation.draft);
  if (!steps.length) return null;
  const names = allocation.draft.planetIds.map(
    (id) => offer.planets.find((planet) => planet.id === id)?.planetName ?? id,
  );
  if (allocation.draft.tradeGoods)
    names.push(`${allocation.draft.tradeGoods} trade good${allocation.draft.tradeGoods === 1 ? "" : "s"}`);
  return { steps, rest: allocation.rest, summary: names.join(", ") };
}
