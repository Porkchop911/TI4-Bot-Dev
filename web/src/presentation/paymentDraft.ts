import type { PendingChoiceDto } from "../protocol/types.ts";
import type { ChoiceRendererModel } from "./choiceModel.ts";
import { getPaymentPayload } from "./choiceModel.ts";

/** One planet the player may exhaust, as offered by the pending payment decision. */
export interface PayablePlanet {
  /** Option id, e.g. `exhaust|jord` or `exhaust|jord|influence`. */
  id: string;
  /** Planet id on the board (`jord`). */
  planetId: string;
  planetName: string;
  worth: number;
  label: string;
  sourceKind?: string;
  /**
   * The planet's own resource value (what exhausting it takes away from production this round).
   * Unknown (`undefined`) counts as 0 in the influence policy, which then reduces to least waste.
   */
  resources?: number;
}

export interface PaymentOffer {
  planets: PayablePlanet[];
  hasTradeGoodOption: boolean;
  tradeGoodWorth: number;
  owed: number;
  totalAmount: number;
  alreadyPaid: number;
  currency: "Resources" | "Influence";
}

export interface PaymentDraft {
  planetIds: string[];
  tradeGoods: number;
}

export const EMPTY_PAYMENT_DRAFT: PaymentDraft = { planetIds: [], tradeGoods: 0 };

/** The planet an `exhaust|planet[|variant]` option id refers to. */
export const paymentPlanetKey = (optionId: string): string =>
  optionId.replace(/^exhaust\|/, "").split("|")[0];

export const PAYMENT_SUBTYPES = new Set([
  "pay_resources",
  "pay_influence",
  "leadership_spend_influence",
]);

/** True for the decisions whose answer is a set of planets (and trade goods) to spend. */
export function isPaymentChoice(choice: PendingChoiceDto | null | undefined): boolean {
  if (!choice) return false;
  const subtype = choice.context?.subtype ?? "";
  if (PAYMENT_SUBTYPES.has(subtype)) return true;
  return choice.options.some((o) => o.id.startsWith("exhaust|")) && subtype !== "vote_exhaust_planet";
}

export function derivePaymentOffer(
  choice: PendingChoiceDto,
  model?: ChoiceRendererModel | null,
  /** Resource value per planet id, when known (feeds the influence Auto-pay policy). */
  resourcesOf?: ReadonlyMap<string, number>,
): PaymentOffer {
  const constraints = model?.outstanding?.[0] ?? choice.context?.outstanding?.[0];
  const totalAmount =
    model?.selectionMode.mode === "quantity"
      ? model.selectionMode.target
      : (constraints?.amount ?? 0);
  const alreadyPaid =
    model?.selectionMode.mode === "quantity" ? model.selectionMode.paid : (constraints?.paid ?? 0);
  const currency =
    model?.selectionMode.mode === "quantity"
      ? model.selectionMode.unit === "influence"
        ? "Influence"
        : "Resources"
      : choice.context?.subtype === "pay_influence" ||
          constraints?.kind?.toLowerCase() === "influence"
        ? "Influence"
        : "Resources";

  const planets: PayablePlanet[] = [];
  let hasTradeGoodOption = false;
  let tradeGoodWorth = 1;
  for (const opt of choice.options) {
    if (opt.id === "decline" || opt.kind === "decline") continue;
    if (opt.id === "trade_good") {
      hasTradeGoodOption = true;
      const p = getPaymentPayload(opt);
      if (p.worth > 0) tradeGoodWorth = p.worth;
      continue;
    }
    if (opt.id.startsWith("exhaust|") || opt.kind === "pay") {
      const p = getPaymentPayload(opt);
      planets.push({
        id: opt.id,
        planetId: paymentPlanetKey(opt.id),
        planetName: p.planetName || opt.label || "Planet",
        worth: p.worth > 0 ? p.worth : 0,
        label: opt.label,
        sourceKind: p.source,
        resources: resourcesOf?.get(paymentPlanetKey(opt.id)),
      });
    }
  }
  return {
    planets,
    hasTradeGoodOption,
    tradeGoodWorth,
    owed: Math.max(0, totalAmount - alreadyPaid),
    totalAmount,
    alreadyPaid,
    currency,
  };
}

/** One planet can be offered as several options; only one variant may be staged at a time. */
export function togglePlanetInDraft(ids: readonly string[], optionId: string): string[] {
  return ids.includes(optionId)
    ? ids.filter((p) => p !== optionId)
    : [...ids.filter((p) => paymentPlanetKey(p) !== paymentPlanetKey(optionId)), optionId];
}

export interface PaymentSummary {
  fromPlanets: number;
  fromTradeGoods: number;
  committed: number;
  shortfall: number;
  surplus: number;
  settled: boolean;
}

export function summarizePayment(offer: PaymentOffer, draft: PaymentDraft): PaymentSummary {
  const fromPlanets = draft.planetIds.reduce(
    (sum, id) => sum + (offer.planets.find((p) => p.id === id)?.worth ?? 0),
    0,
  );
  const fromTradeGoods = draft.tradeGoods * offer.tradeGoodWorth;
  const committed = fromPlanets + fromTradeGoods;
  const shortfall = Math.max(0, offer.owed - committed);
  return {
    fromPlanets,
    fromTradeGoods,
    committed,
    shortfall,
    surplus: Math.max(0, committed - offer.owed),
    // A partial payment would leave the engine asking again, so only a fully staged bill confirms.
    settled: offer.owed > 0 && shortfall === 0,
  };
}

/** What stops the player from paying right now, in plain words; null when ready. */
export function paymentProblem(
  offer: PaymentOffer,
  draft: PaymentDraft,
  tradeGoodsAvailable: number,
): string | null {
  if (offer.owed <= 0) return "Nothing is owed.";
  const s = summarizePayment(offer, draft);
  if (s.settled) return null;
  const unit = offer.currency.toLowerCase();
  const best = bestPayableTotal(offer, tradeGoodsAvailable);
  if (best < offer.owed)
    return `Not enough: all offered planets and trade goods cover only ${best} of ${offer.owed} ${unit}.`;
  return `Short by ${s.shortfall} ${unit}: stage more planets or trade goods, or use Auto-pay.`;
}

function bestPayableTotal(offer: PaymentOffer, tradeGoodsAvailable: number): number {
  const byPlanet = new Map<string, number>();
  for (const p of offer.planets) byPlanet.set(p.planetId, Math.max(byPlanet.get(p.planetId) ?? 0, p.worth));
  let total = 0;
  for (const w of byPlanet.values()) total += w;
  return total + (offer.hasTradeGoodOption ? tradeGoodsAvailable * offer.tradeGoodWorth : 0);
}

/** Resource payments: least overshoot, then the fewest planets (unchanged). */
function pickLeastWaste(planets: PayablePlanet[], owed: number): string[] | null {
  // Subset search with a per-sum table: least overshoot, then fewest planets.
  const cap = owed + Math.max(0, ...planets.map((p) => p.worth));
  const best = new Map<number, string[]>([[0, []]]);
  for (const p of planets) {
    for (const [sum, ids] of [...best.entries()]) {
      const next = Math.min(sum + p.worth, cap);
      const existing = best.get(next);
      if (!existing || ids.length + 1 < existing.length) best.set(next, [...ids, p.id]);
    }
  }
  let pick: { sum: number; ids: string[] } | null = null;
  for (const [sum, ids] of best) {
    if (sum < owed) continue;
    if (!pick || sum < pick.sum || (sum === pick.sum && ids.length < pick.ids.length))
      pick = { sum, ids };
  }
  return pick ? pick.ids : null;
}

const MAX_EXACT_PLANETS = 18;

/**
 * Influence payments: "first use planets with no resources, then do not waste resources".
 * Among the planet sets that cover the bill, pick the one that, in this order,
 *  1. exhausts the fewest RESOURCES in total (so pure-influence planets, 0 resources, come first
 *     and are used even when that overpays more than a resource planet would),
 *  2. then wastes the least influence (overpay),
 *  3. then uses the fewest planets,
 *  4. then the lexicographically smallest planet ids (deterministic).
 * Sets with a spare item are never best (dropping it lowers waste at no resource cost), so the
 * engine's "stops asking once covered" rule holds. Returns the option ids, or null when the planets
 * cannot cover the bill. Unknown resources count as 0.
 */
export function pickInfluencePlanets(planets: PayablePlanet[], owed: number): string[] | null {
  const sorted = [...planets].sort((a, b) => a.planetId.localeCompare(b.planetId));
  if (sorted.length > MAX_EXACT_PLANETS) return pickLeastWaste(sorted, owed);
  let best: { resources: number; waste: number; count: number; key: string; ids: string[] } | null = null;
  for (let mask = 1; mask < 1 << sorted.length; mask += 1) {
    let sum = 0;
    let resources = 0;
    let count = 0;
    for (let i = 0; i < sorted.length; i += 1) {
      if (mask & (1 << i)) {
        sum += sorted[i].worth;
        resources += sorted[i].resources ?? 0;
        count += 1;
      }
    }
    if (sum < owed) continue;
    const waste = sum - owed;
    const ids = sorted.filter((_, i) => mask & (1 << i)).map((p) => p.id);
    const key = ids.join("\u0000");
    if (
      !best ||
      resources < best.resources ||
      (resources === best.resources &&
        (waste < best.waste ||
          (waste === best.waste && (count < best.count || (count === best.count && key < best.key)))))
    )
      best = { resources, waste, count, key, ids };
  }
  return best ? best.ids : null;
}

/**
 * Suggests what to exhaust. Resource payments: the planets that cover the bill with the least waste
 * (then the fewest planets). Influence payments: see `pickInfluencePlanets` (planets with no
 * resources first, then not wasting resources). Trade goods only when the planets cannot cover it. It only stages the
 * suggestion; nothing is paid until the player confirms. When the bill cannot be covered the
 * result is the best effort and `settled` is false.
 */
export function suggestAutoPay(
  offer: PaymentOffer,
  tradeGoodsAvailable: number,
): PaymentDraft & { settled: boolean } {
  // One variant per planet: the one paying in the owed currency, else the most valuable.
  const perPlanet = new Map<string, PayablePlanet>();
  for (const p of offer.planets) {
    if (p.worth <= 0) continue;
    const cur = perPlanet.get(p.planetId);
    const native = (x: PayablePlanet) =>
      !x.sourceKind || x.sourceKind.toLowerCase() === offer.currency.toLowerCase();
    if (!cur || (native(p) && !native(cur)) || (native(p) === native(cur) && p.worth > cur.worth))
      perPlanet.set(p.planetId, p);
  }
  const planets = [...perPlanet.values()];
  const owed = offer.owed;
  const tgMax = offer.hasTradeGoodOption ? Math.max(0, tradeGoodsAvailable) : 0;
  if (owed <= 0) return { planetIds: [], tradeGoods: 0, settled: false };

  const pick =
    offer.currency === "Influence"
      ? pickInfluencePlanets(planets, owed)
      : pickLeastWaste(planets, owed);
  if (pick) return { planetIds: pick, tradeGoods: 0, settled: true };

  // Planets alone fall short: take them all, top up with trade goods.
  const all = planets.map((p) => p.id);
  const total = planets.reduce((s, p) => s + p.worth, 0);
  const gap = owed - total;
  const needTg = Math.ceil(gap / offer.tradeGoodWorth);
  const tradeGoods = Math.min(tgMax, needTg);
  return { planetIds: all, tradeGoods, settled: needTg <= tgMax };
}

/** The option a map click on `planetId` stands for: the variant paying in the owed currency. */
export function paymentOptionForPlanet(offer: PaymentOffer, planetId: string): PayablePlanet | null {
  const variants = offer.planets.filter((p) => p.planetId === planetId);
  if (variants.length === 0) return null;
  const native = variants.find(
    (p) => !p.sourceKind || p.sourceKind.toLowerCase() === offer.currency.toLowerCase(),
  );
  return native ?? variants[0];
}

/** What a payable planet shows on the map. */
export interface PaymentMark {
  planetId: string;
  optionId: string;
  worth: number;
  unit: "R" | "I";
  staged: boolean;
  /** The planet's own resource value, when known (shown beside the payment worth). */
  resources?: number;
  /** Diplomacy: the planet is picked to be readied, not exhausted (worth is its influence). */
  readies?: boolean;
}

export function derivePaymentMarks(offer: PaymentOffer, draft: PaymentDraft): Map<string, PaymentMark> {
  const marks = new Map<string, PaymentMark>();
  for (const p of offer.planets) {
    const stagedVariant = draft.planetIds.find((id) => paymentPlanetKey(id) === p.planetId);
    const shown = stagedVariant
      ? (offer.planets.find((x) => x.id === stagedVariant) ?? p)
      : (paymentOptionForPlanet(offer, p.planetId) ?? p);
    marks.set(p.planetId, {
      planetId: p.planetId,
      optionId: shown.id,
      worth: shown.worth,
      unit: offer.currency === "Influence" ? "I" : "R",
      staged: Boolean(stagedVariant),
      resources: shown.resources,
    });
  }
  return marks;
}

export type PaymentStep = { kind: "exhaust"; planet: string } | { kind: "trade_good" };

/**
 * The server-side steps for a staged payment. When only two payment options remain and the first
 * does not settle the bill, the engine spends the sole remaining option without offering a second
 * choice, so that automatic spend is not part of the plan; spends beyond what the bill needs are
 * dropped too.
 */
export function buildPaymentSteps(
  choice: PendingChoiceDto,
  offer: PaymentOffer,
  draft: PaymentDraft,
): PaymentStep[] {
  const worthOf = (id: string) => offer.planets.find((p) => p.id === id)?.worth ?? 0;
  const autoSpendsLast =
    choice.options.filter((option) => option.kind !== "decline").length === 2 &&
    draft.planetIds.length === 2 &&
    draft.tradeGoods === 0 &&
    worthOf(draft.planetIds[0]) < offer.owed;
  let remaining = offer.owed;
  const planetsToSubmit = (autoSpendsLast ? draft.planetIds.slice(0, 1) : draft.planetIds).filter(
    (id) => {
      if (remaining <= 0) return false;
      remaining -= worthOf(id);
      return true;
    },
  );
  return [
    ...planetsToSubmit.map((id) => ({
      kind: "exhaust" as const,
      planet: id.replace(/^exhaust\|/, ""),
    })),
    ...Array.from(
      { length: Math.min(draft.tradeGoods, Math.ceil(Math.max(0, remaining) / offer.tradeGoodWorth)) },
      () => ({ kind: "trade_good" as const }),
    ),
  ];
}
