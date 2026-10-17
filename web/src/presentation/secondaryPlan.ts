import type {
  BoardView,
  ChoiceOption,
  PendingChoiceDto,
  PlayerView,
} from "../protocol/types.ts";
import { getTechnologyMeta, findPlanetMeta } from "../protocol/contentCatalog.ts";
import {
  describeCommandTokens,
  maxPurchases,
  paymentCheck,
  planPayment,
  tokenOutcome,
  TOKEN_POOLS,
  type PaymentOverride,
  type Pools,
  type TokenStep,
  type TokenStaging,
} from "./commandTokens.ts";
import {
  TECH_TIERS,
  UNIT_UPGRADE_TECH_IDS,
  TECH_TRACKS,
  checkTechPrerequisites,
  getFactionTechIds,
  hasResearchedTech,
  hydrateTech,
  isTechAllowedForFaction,
  techMatches,
} from "./technologyData.ts";
import { cardFamily, cardName, type CardFamily } from "./strategicAction.ts";
import {
  buildPaymentSteps,
  derivePaymentOffer,
  paymentPlanetKey,
  summarizePayment,
  type PaymentStep,
} from "./paymentDraft.ts";
import { planBuilds } from "./productionDraft.ts";

/**
 * A prepared strategy-card secondary: the follower's private, revocable suggestion.
 *
 * It is stored by meaning (card plus intent), never by option id: the ids and labels of the real
 * question differ per situation (a faction waiver replaces the plain "yes" for a seat without a
 * token). Nothing here spends or exhausts anything; the real decision is still answered by the
 * player (or, in auto mode, by a click-equivalent submission) when the engine asks.
 */
export interface SecondaryPlan {
  /** Strategy card id, e.g. `pok7technology`. */
  card: string;
  /** Follow the secondary (`true`) or skip it (`false`). */
  follow: boolean;
  /** Technology: the technology to research. */
  tech?: string;
  /** Diplomacy: up to two exhausted planets to ready, in order. */
  planets?: string[];
  /** Construction: what to place and where. */
  structure?: { unit: StructureUnit; planet: string };
  /**
   * Leadership: the tokens to buy with influence and the pool each one goes to, and the EXACT
   * payment (planets to exhaust by purchase-planet id, trade goods to spend) chosen with the map
   * payment. Without `payment` (an older plan) the payment is planned by Auto-pay when the real
   * question opens.
   */
  leadership?: { pools: Pools; payment?: LeadershipPayment };
  /**
   * Warfare: what to build at home, as unit ids in the order the production builder stages them
   * (one entry per build batch, e.g. `["infantry", "infantry", "carrier"]`), and how to pay for the
   * FIRST build when the engine asks (planets to exhaust by planet id, trade goods to spend). Later
   * builds are paid by hand: their payment questions depend on what the first one left.
   */
  production?: { builds: string[]; payment?: { planets: string[]; tradeGoods: number } };
}

/** A Leadership payment as chosen: the planets exhausted and the trade goods spent. */
export interface LeadershipPayment {
  planets: string[];
  tradeGoods: number;
}

export type StructureUnit = "pds" | "spacedock";
export const STRUCTURE_UNITS: readonly StructureUnit[] = ["pds", "spacedock"];

/** A plan as kept on the device, with what it is valid for. */
export interface StoredPlan {
  v: 1;
  /** `StrategicAction.key` of the action it was made for. */
  actionKey: string;
  /** History generation it was made in (diagnostic only: batch commits bump it mid-action). */
  generation: number;
  plan: SecondaryPlan;
}

const isString = (value: unknown): value is string => typeof value === "string" && value.length > 0;

/** Parses a stored plan strictly; anything unexpected is `null` (the plan is then simply gone). */
export function parseStoredPlan(raw: string | null | undefined): StoredPlan | null {
  if (!raw) return null;
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!data || typeof data !== "object") return null;
  const record = data as Record<string, unknown>;
  const plan = record.plan as Record<string, unknown> | undefined;
  if (record.v !== 1 || !isString(record.actionKey) || typeof record.generation !== "number") {
    return null;
  }
  if (!plan || typeof plan !== "object" || !isString(plan.card) || typeof plan.follow !== "boolean") {
    return null;
  }
  const clean: SecondaryPlan = { card: plan.card, follow: plan.follow };
  if (isString(plan.tech)) clean.tech = plan.tech;
  if (Array.isArray(plan.planets)) {
    const planets = plan.planets.filter(isString).slice(0, 2);
    if (planets.length) clean.planets = planets;
  }
  const structure = plan.structure as Record<string, unknown> | undefined;
  if (
    structure &&
    STRUCTURE_UNITS.includes(structure.unit as StructureUnit) &&
    isString(structure.planet)
  ) {
    clean.structure = { unit: structure.unit as StructureUnit, planet: structure.planet };
  }
  const leadership = plan.leadership as Record<string, unknown> | undefined;
  const pools = leadership?.pools as Record<string, unknown> | undefined;
  if (pools) {
    const counts = TOKEN_POOLS.map((pool) => pools[pool] ?? 0);
    const valid = counts.every((count) => typeof count === "number" && Number.isInteger(count) && count >= 0);
    const total = valid ? (counts as number[]).reduce((sum, count) => sum + count, 0) : 0;
    if (valid && total >= 1 && total <= 10) {
      clean.leadership = {
        pools: { tactic: counts[0] as number, fleet: counts[1] as number, strategic: counts[2] as number },
      };
      const payment = leadership?.payment as Record<string, unknown> | undefined;
      if (payment && Array.isArray(payment.planets)) {
        const planets = payment.planets.filter(isString).slice(0, 24);
        const goods = payment.tradeGoods;
        if (typeof goods === "number" && Number.isInteger(goods) && goods >= 0 && goods <= 60) {
          clean.leadership.payment = { planets, tradeGoods: goods };
        }
      }
    }
  }
  const production = plan.production as Record<string, unknown> | undefined;
  if (production && Array.isArray(production.builds)) {
    const builds = production.builds.filter(isString).slice(0, MAX_BUILDS);
    if (builds.length) {
      clean.production = { builds };
      const payment = production.payment as Record<string, unknown> | undefined;
      if (payment && Array.isArray(payment.planets)) {
        const planets = payment.planets.filter(isString).slice(0, 12);
        const tradeGoods = payment.tradeGoods;
        if (typeof tradeGoods === "number" && Number.isInteger(tradeGoods) && tradeGoods >= 0 && tradeGoods <= 30) {
          if (planets.length || tradeGoods) clean.production.payment = { planets, tradeGoods };
        }
      }
    }
  }
  return { v: 1, actionKey: record.actionKey, generation: record.generation, plan: clean };
}

/** Most build batches a prepared Warfare plan may hold (the engine's production limit is far lower). */
const MAX_BUILDS = 24;

/** Tokens a Leadership plan buys. */
export const leadershipTokens = (pools: Pools): number =>
  TOKEN_POOLS.reduce((sum, pool) => sum + pools[pool], 0);

export const serializeStoredPlan = (stored: StoredPlan): string => JSON.stringify(stored);

/** Drops the parts of a plan that do not belong to its card, so a card switch never leaks intent. */
export function normalizePlan(plan: SecondaryPlan): SecondaryPlan {
  const family = cardFamily(plan.card);
  const next: SecondaryPlan = { card: plan.card, follow: plan.follow };
  if (!plan.follow) return next;
  if (family === "technology" && plan.tech) next.tech = plan.tech;
  if (family === "diplomacy" && plan.planets?.length) next.planets = plan.planets.slice(0, 2);
  if (family === "construction" && plan.structure) next.structure = plan.structure;
  if (family === "leadership" && plan.leadership) next.leadership = plan.leadership;
  if (family === "warfare" && plan.production?.builds.length) next.production = plan.production;
  return next;
}

/** Trade's owner may replenish a follower, which removes the follower's secondary entirely. */
export const TRADE_WARNING =
  "If the Trade player replenishes you, you are not offered a secondary at all and this is dropped.";

export const hasTradeWarning = (card: string): boolean => cardFamily(card) === "trade";

/** Whether the card has a second-level choice the panel can prepare (beyond follow or skip). */
export function hasDetailStep(family: CardFamily | null): boolean {
  return (
    family === "technology" ||
    family === "diplomacy" ||
    family === "construction" ||
    family === "leadership" ||
    family === "warfare"
  );
}

// ---------------------------------------------------------------------------------------------
// What the panel can offer, from the viewer's own public data. Estimates: the engine re-checks.
// ---------------------------------------------------------------------------------------------

/** Technologies the seat can probably research now: not owned, allowed, prerequisites met. */
export function researchableEstimate(player: PlayerView | undefined): string[] {
  if (!player) return [];
  const counts = { PROPULSION: 0, BIOTIC: 0, CYBERNETIC: 0, WARFARE: 0 };
  for (const track of TECH_TRACKS) {
    counts[track.id] = (player.technologies ?? []).filter((owned) =>
      track.techIds.some((id) => techMatches(owned, id)),
    ).length;
  }
  const none = { PROPULSION: 0, BIOTIC: 0, CYBERNETIC: 0, WARFARE: 0 };
  const candidates = new Set<string>([
    ...TECH_TIERS.flatMap((tier) => tier.rows.flat()),
    ...UNIT_UPGRADE_TECH_IDS,
    ...getFactionTechIds(player.faction),
  ]);
  return [...candidates].filter((id) => {
    if (hasResearchedTech(player, id)) return false;
    const tech = hydrateTech(id);
    if (!isTechAllowedForFaction(tech, player.faction)) return false;
    return checkTechPrerequisites(tech, counts, none, player.faction);
  });
}

export interface OwnPlanet {
  planet: string;
  system: string;
  exhausted: boolean;
}

/** Planets the seat controls, from the public board. */
export function ownPlanets(board: BoardView | null | undefined, seat: string | null | undefined): OwnPlanet[] {
  if (!board || !seat) return [];
  const seen = new Set<string>();
  const planets: OwnPlanet[] = [];
  for (const [systemId, system] of Object.entries(board.systems ?? {})) {
    for (const planet of Object.values(system.planets ?? {})) {
      if (planet.controlled_by !== seat || seen.has(planet.planet_id)) continue;
      seen.add(planet.planet_id);
      planets.push({ planet: planet.planet_id, system: systemId, exhausted: Boolean(planet.exhausted) });
    }
  }
  return planets;
}

export const planetName = (planet: string): string => findPlanetMeta(planet)?.name ?? planet;

/** A planet's name for lists: the catalog name, else its id as words ("dal_bootha" -> "Dal Bootha"). */
export const planetLabel = (planet: string): string => {
  const known = findPlanetMeta(planet)?.name;
  return known ?? planet.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
};
export const techName = (tech: string): string => getTechnologyMeta(tech).name;
export const structureName = (unit: StructureUnit): string => (unit === "pds" ? "PDS" : "space dock");

// ---------------------------------------------------------------------------------------------
// Resolving a plan against the real question
// ---------------------------------------------------------------------------------------------

export type StepResolution =
  /** The plan has nothing to say about this decision (for example a faction prompt in between). */
  | { kind: "none" }
  | { kind: "option"; optionId: string; text: string }
  | { kind: "tokens"; steps: TokenStep[]; text: string; prefill?: TokenPrefill }
  /**
   * Warfare: the planned builds, validated against the real production question. Sent through the
   * production builder's own path (one batch for a single build, the build queue for several).
   */
  | { kind: "production"; destination: string; units: string[]; text: string }
  /** Warfare: the prepared payment for the first build, as the payment batch's steps. */
  | { kind: "payment"; steps: PaymentStep[]; text: string }
  /** Diplomacy: the planets to ready, validated against the exhausted planets offered now (sent as one batch). */
  | { kind: "ready"; planetIds: string[]; text: string }
  /**
   * The plan no longer validates: show "Needs review" and answer nothing. A Leadership plan whose
   * payment no longer fits carries the replacement the usual UI is opened with (`prefill`).
   */
  | { kind: "review"; reason: string; prefill?: TokenPrefill; ready?: string[] };

/** What the token panel is opened with: the bought tokens, their pools and the payment chosen. */
export interface TokenPrefill {
  bought: number;
  pools: Pools;
  /** `null`: Auto-pay's plan. */
  override: PaymentOverride | null;
}

const isNoOption = (option: ChoiceOption) =>
  option.id === "no" || option.id === "decline" || option.kind === "decline";
const isYesOption = (option: ChoiceOption) => option.id === "yes" || option.id === "follow";
const isWaiver = (option: ChoiceOption) => option.id.startsWith("follow|waived|");

/** True for the strategy-card secondary question itself. */
export const isSecondaryQuestion = (choice: Pick<PendingChoiceDto, "details">): boolean =>
  choice.details?.kind === "strategy_secondary";

/**
 * The prepared answer for one decision of the viewer's, validated against the options the engine
 * really offers. Pure; the caller decides whether to show, prefill or (auto mode) submit it.
 */
export function resolveStep(
  plan: SecondaryPlan | null | undefined,
  choice: PendingChoiceDto | null | undefined,
  viewerSeat: string | null | undefined,
  /** Steps of this plan already sent (a production's later offers belong to its own build queue). */
  done: ReadonlySet<string> = NOTHING_DONE,
  /** Resource value per planet id: feeds the influence Auto-pay policy of a Leadership purchase. */
  resourcesOf?: ReadonlyMap<string, number>,
): StepResolution {
  if (!plan || !choice || !viewerSeat || choice.actor !== viewerSeat) return { kind: "none" };
  const family = cardFamily(plan.card);
  const name = cardName(plan.card);
  if (isSecondaryQuestion(choice)) {
    if (choice.details?.card !== plan.card) {
      return { kind: "review", reason: `A different card's secondary is open (you prepared ${name}).` };
    }
    if (!plan.follow) {
      const no = choice.options.find(isNoOption);
      return no
        ? { kind: "option", optionId: no.id, text: `Skip the ${name} secondary` }
        : { kind: "review", reason: "Skipping is not offered any more." };
    }
    if (family === "leadership" && plan.leadership) return resolveLeadership(plan, choice, name, resourcesOf);
    const yes = choice.options.find(isYesOption);
    if (yes) {
      const spends = choice.details?.costs_token !== false;
      return {
        kind: "option",
        optionId: yes.id,
        text: `Follow ${name}${spends ? " (spends 1 strategy token)" : ""}`,
      };
    }
    const waivers = choice.options.filter(isWaiver);
    if (waivers.length === 1) {
      return {
        kind: "option",
        optionId: waivers[0].id,
        text: `Follow ${name} for free: ${waivers[0].label}`,
      };
    }
    if (waivers.length > 1) {
      return { kind: "review", reason: "Several free ways to follow are offered; choose one." };
    }
    return { kind: "review", reason: `You can no longer follow ${name}.` };
  }
  if (!plan.follow) return { kind: "none" };
  const subtype = choice.context?.subtype;
  if (subtype === "research_technology" && plan.tech) {
    const option = choice.options.find((o) => o.kind === "research" && techMatches(o.id, plan.tech!));
    return option
      ? { kind: "option", optionId: option.id, text: `Research ${techName(plan.tech)}` }
      : { kind: "review", reason: `${techName(plan.tech)} can no longer be researched.` };
  }
  if (subtype === "ready_planet" && plan.planets?.length) {
    const offered = new Set(choice.options.filter((o) => o.kind === "ready").map((o) => o.id));
    // The engine asks until two planets are readied (or nothing is left): both are answered together.
    const required = Math.min(2, offered.size);
    const picks = [...new Set(plan.planets)].filter((planet) => offered.has(planet));
    if (required > 0 && picks.length >= required) {
      const planetIds = picks.slice(0, required);
      return { kind: "ready", planetIds, text: `Ready ${planetIds.map(planetName).join(" and ")}` };
    }
    return {
      kind: "review",
      reason: picks.length
        ? `Only ${picks.length} of the ${required} planets to ready is still exhausted; pick the rest by hand.`
        : "No prepared planet is offered (readied already, or no longer exhausted); choose by hand.",
      ...(picks.length ? { ready: picks } : {}),
    };
  }
  if (subtype === "produce_unit" && plan.production?.builds.length && !done.has("production")) {
    const planned = planBuilds(choice, plan.production.builds);
    return planned.ok
      ? {
          kind: "production",
          destination: planned.destination,
          units: plan.production.builds,
          text: `Build ${describeBuilds(plan.production.builds)}`,
        }
      : { kind: "review", reason: planned.reason };
  }
  if (
    subtype === "pay_resources" &&
    plan.production?.payment &&
    done.has("production") &&
    !done.has("payment")
  ) {
    return resolveProductionPayment(plan.production.payment, choice);
  }
  if (subtype === "place_structure" && plan.structure) {
    const { unit, planet } = plan.structure;
    const option = choice.options.find((o) => {
      const [kind, , target, extra] = o.id.split("|");
      return kind === unit && target === planet && extra === undefined;
    });
    return option
      ? {
          kind: "option",
          optionId: option.id,
          text: `Place a ${structureName(unit)} on ${planetName(planet)}`,
        }
      : {
          kind: "review",
          reason: `A ${structureName(unit)} can no longer be placed on ${planetName(planet)}.`,
        };
  }
  return { kind: "none" };
}

const NOTHING_DONE: ReadonlySet<string> = new Set();

/** "2 x infantry, carrier" for planned build batches. */
export function describeBuilds(builds: readonly string[]): string {
  const counts = new Map<string, number>();
  for (const build of builds) {
    // A staged exchange (`exchange|carrier`) is a different purchase from the paid build.
    const unit = build.startsWith("exchange|") ? `${build.slice("exchange|".length)} (exchange)` : build;
    counts.set(unit, (counts.get(unit) ?? 0) + 1);
  }
  return [...counts].map(([unit, n]) => (n > 1 ? `${n} x ${unit}` : unit)).join(", ");
}

/** The prepared payment against the payment question the first build opened. */
function resolveProductionPayment(
  wanted: { planets: string[]; tradeGoods: number },
  choice: PendingChoiceDto,
): StepResolution {
  const offer = derivePaymentOffer(choice);
  const planetIds: string[] = [];
  for (const planet of wanted.planets) {
    const offered = offer.planets.find((candidate) => candidate.planetId === planet);
    if (!offered) {
      return { kind: "review", reason: `${planetName(planet)} can no longer be exhausted to pay.` };
    }
    planetIds.push(offered.id);
  }
  if (wanted.tradeGoods > 0 && !offer.hasTradeGoodOption) {
    return { kind: "review", reason: "Trade goods can no longer be spent on this payment." };
  }
  const draft = { planetIds, tradeGoods: wanted.tradeGoods };
  if (!summarizePayment(offer, draft).settled) {
    return { kind: "review", reason: "The prepared payment no longer covers what is owed." };
  }
  const steps = buildPaymentSteps(choice, offer, draft);
  const exhausted = steps.filter((step) => step.kind === "exhaust");
  const goods = steps.filter((step) => step.kind === "trade_good").length;
  const parts = [
    ...exhausted.map((step) => planetName(paymentPlanetKey(step.planet))),
    ...(goods ? [`${goods} trade good${goods === 1 ? "" : "s"}`] : []),
  ];
  return { kind: "payment", steps, text: `Pay with ${parts.join(", ") || "what is offered"}` };
}

function resolveLeadership(
  plan: SecondaryPlan,
  choice: PendingChoiceDto,
  name: string,
  resourcesOf?: ReadonlyMap<string, number>,
): StepResolution {
  const wanted = plan.leadership!;
  const count = leadershipTokens(wanted.pools);
  const view = describeCommandTokens(choice, true, resourcesOf);
  if (!view?.purchase) {
    // No purchase details: fall back to the plain first purchase; later prompts are answered by hand.
    const yes = choice.options.find(isYesOption);
    return yes
      ? { kind: "option", optionId: yes.id, text: `Follow ${name}` }
      : { kind: "review", reason: "You can no longer buy a command token." };
  }
  const affordable = maxPurchases(view);
  if (affordable < count) {
    return {
      kind: "review",
      reason:
        affordable === 0
          ? "You can no longer afford a command token."
          : `You can now afford only ${affordable} of the ${count} tokens you prepared.`,
    };
  }
  const staging: TokenStaging = { ...wanted.pools };
  const tokens = `${count} command token${count === 1 ? "" : "s"}`;
  const auto = planPayment(view, count);
  const autoPrefill: TokenPrefill | undefined = auto
    ? { bought: count, pools: wanted.pools, override: null }
    : undefined;
  const autoNote = auto
    ? ` Auto-pay would use ${describeSpend(auto.planets.map((planet) => planet.id), auto.tradeGoods)}; it is selected for you to confirm.`
    : "";
  const prepared = wanted.payment;
  if (prepared) {
    // The payment chosen while preparing, checked against the planets and goods offered now.
    const gone = prepared.planets.find(
      (planet) => !view.purchase!.planets.some((offered) => offered.id === planet),
    );
    if (gone !== undefined) {
      return {
        kind: "review",
        reason: `${planetName(gone)} can no longer pay (it is exhausted or no longer yours).${autoNote}`,
        prefill: autoPrefill,
      };
    }
    if (prepared.tradeGoods > view.purchase.tradeGoods) {
      return {
        kind: "review",
        reason: `You now hold ${view.purchase.tradeGoods} trade good${view.purchase.tradeGoods === 1 ? "" : "s"}, not the ${prepared.tradeGoods} you planned to spend.${autoNote}`,
        prefill: autoPrefill,
      };
    }
    const override: PaymentOverride = { planetIds: prepared.planets, tradeGoods: prepared.tradeGoods };
    const check = paymentCheck(view, count, override);
    if (check.problem !== null) {
      return {
        kind: "review",
        reason: `The prepared payment no longer works: ${check.problem}${autoNote}`,
        prefill: autoPrefill,
      };
    }
    const outcome = tokenOutcome(view, staging, count, override);
    if (!outcome || outcome.kind !== "plan") {
      return { kind: "review", reason: `The prepared payment can no longer be sent.${autoNote}`, prefill: autoPrefill };
    }
    return {
      kind: "tokens",
      steps: outcome.steps,
      text: `Buy ${tokens} (${describePools(wanted.pools)}) for ${view.purchase.cost * count} influence, paying ${describeSpend(prepared.planets, prepared.tradeGoods)}`,
      prefill: { bought: count, pools: wanted.pools, override },
    };
  }
  const outcome = tokenOutcome(view, staging, count);
  if (!outcome || outcome.kind !== "plan") {
    return { kind: "review", reason: "The influence payment can no longer be planned." };
  }
  return {
    kind: "tokens",
    steps: outcome.steps,
    text: `Buy ${tokens} (${describePools(wanted.pools)}) for ${view.purchase.cost * count} influence`,
    prefill: autoPrefill,
  };
}

/** "Jord, Lodor, 2 trade goods" for a payment's planets and goods. */
export function describeSpend(planets: readonly string[], tradeGoods: number): string {
  const parts = [
    ...planets.map(planetName),
    ...(tradeGoods ? [`${tradeGoods} trade good${tradeGoods === 1 ? "" : "s"}`] : []),
  ];
  return parts.join(", ") || "nothing";
}

/** "2 tactic, 1 fleet" for the pools a purchase fills. */
export const describePools = (pools: Pools): string =>
  TOKEN_POOLS.filter((pool) => pools[pool] > 0)
    .map((pool) => `${pools[pool]} ${pool === "strategic" ? "strategy" : pool}`)
    .join(", ");

/** One-line description of a plan for the badge and the toast. */
export function describePlan(plan: SecondaryPlan): string {
  const name = cardName(plan.card);
  if (!plan.follow) return `Skip ${name}`;
  const family = cardFamily(plan.card);
  if (family === "technology" && plan.tech) return `Follow ${name}: research ${techName(plan.tech)}`;
  if (family === "diplomacy" && plan.planets?.length) {
    return `Follow ${name}: ready ${plan.planets.map(planetName).join(" and ")}`;
  }
  if (family === "construction" && plan.structure) {
    return `Follow ${name}: ${structureName(plan.structure.unit)} on ${planetName(plan.structure.planet)}`;
  }
  if (family === "warfare" && plan.production?.builds.length) {
    return `Follow ${name}: build ${describeBuilds(plan.production.builds)}`;
  }
  if (family === "leadership" && plan.leadership) {
    const count = leadershipTokens(plan.leadership.pools);
    const paid = plan.leadership.payment
      ? ` paying ${describeSpend(plan.leadership.payment.planets, plan.leadership.payment.tradeGoods)}`
      : "";
    return `Buy ${count} command token${count === 1 ? "" : "s"} (${describePools(plan.leadership.pools)})${paid}`;
  }
  return `Follow ${name}`;
}
