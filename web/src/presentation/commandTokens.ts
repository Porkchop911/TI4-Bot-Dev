import type { CommandPools, FleetSupplyHints, PendingChoiceDto } from "../protocol/types.ts";
import { suggestAutoPay, type PayablePlanet, type PaymentDraft, type PaymentOffer } from "./paymentDraft.ts";

const GAIN_PROMPT = "gain a command token into which pool";

/**
 * The headline for a command-token gain. The engine's prompt names a single token; the real
 * count is in the display-only details. Any other prompt (or a gain without a count) is
 * returned unchanged.
 */
export function gainTokensPrompt(choice: Pick<PendingChoiceDto, "prompt" | "details">): string {
  const details = choice.details;
  if (choice.prompt !== GAIN_PROMPT || details?.kind !== "command_tokens" || details.mode !== "gain") {
    return choice.prompt;
  }
  const count = details.tokens_to_place;
  if (typeof count !== "number" || !Number.isFinite(count) || count < 1) return choice.prompt;
  return count === 1
    ? "Gain 1 command token — choose its pool"
    : `Gain ${Math.floor(count)} command tokens — assign them to pools`;
}

export type TokenPool = "tactic" | "fleet" | "strategic";
export const TOKEN_POOLS: readonly TokenPool[] = ["tactic", "fleet", "strategic"];
export type Pools = Record<TokenPool, number>;

export const POOL_LABEL: Record<TokenPool, string> = {
  tactic: "Tactic",
  fleet: "Fleet",
  strategic: "Strategy",
};

/** What each pool is for, shown under its name. */
export const POOL_PURPOSE: Record<TokenPool, string> = {
  tactic: "activate systems",
  fleet: "fleet supply, moves ships",
  strategic: "strategy secondaries and abilities",
};

/**
 * The fewest fleet tokens any arrangement the engine offers keeps. Two for most factions (a fleet
 * pool of 0 or 1 leaves them unable to move); Letnev's Armada may go lower, so this reads it from
 * the offered arrangements instead of assuming a number.
 */
export function minFleetOffered(view: CommandTokenView): number {
  let min = Number.POSITIVE_INFINITY;
  for (const id of view.arrangements) {
    const fleet = Number(id.split("|")[1]);
    if (Number.isFinite(fleet)) min = Math.min(min, fleet);
  }
  return Number.isFinite(min) ? min : 0;
}

const gainOptionId = (pool: TokenPool) => `${pool}_tokens`;

/** One ready planet that can pay influence, as the engine lists it. */
export interface PurchasePlanet {
  id: string;
  /** Influence it pays. */
  worth: number;
  /** Its own resource value (what exhausting it takes from production); null when not known. */
  resources: number | null;
}

/**
 * Leadership's influence purchase, from the question's display-only `purchase` details: each
 * token costs `cost` influence, `max` is how many the seat's influence can pay for in all.
 */
export interface PurchaseView {
  cost: number;
  /** Influence the seat can spend now (planets plus trade goods). */
  influence: number;
  max: number;
  planets: PurchasePlanet[];
  tradeGoods: number;
  tradeGoodWorth: number;
}

export interface CommandTokenView {
  /**
   * "gain": add new tokens to the pools; "redistribute": rearrange the tokens already held in one
   * decision; "restack": rearrange them too, but the engine takes one token move per question
   * (Predictive Intelligence), so the plan goes out as a sequence of moves.
   */
  mode: "gain" | "redistribute" | "restack";
  /** The pools as the engine has them now. */
  current: Pools;
  /** Tokens left in reinforcements. */
  reinforcements: number | null;
  /** Gain: tokens to place in this window. Redistribute: every token held, gained ones included. */
  total: number;
  /**
   * Redistribute: tokens gained in this window (the status phase) and not yet in `current`, so
   * `total` is the held tokens plus these. 0 otherwise.
   */
  gained: number;
  /** Redistribute: the arrangement option ids the engine offers. Restack: the move ids offered now. */
  arrangements: ReadonlySet<string>;
  /** Gain: the influence purchase that follows (or is asked first), planned on the same screen. */
  purchase: PurchaseView | null;
  /** Extra non-fighter ships per system the seat's fleet supply allows (Letnev's Armada: 2), else 0. */
  fleetSupplyBonus: number;
}

/** What Armada adds, shown next to the fleet pool and on the player card. */
export const ARMADA_BONUS = 2;
export const ARMADA_TEXT = `Armada: +${ARMADA_BONUS} fleet supply (pool + ${ARMADA_BONUS} non-fighter ships per system)`;

/** Whether a seat's faction id is Letnev (Armada applies to it unconditionally). */
export const hasArmada = (faction: string | null | undefined): boolean =>
  (faction ?? "").toLowerCase().replace(/[-_\s]/g, "") === "letnev";

/** Staged tokens per pool: added ones for a gain, the whole arrangement for a redistribute. */
export type TokenStaging = Pools;

const asCount = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : null;

/**
 * A command-token decision as pools to fill, from the server's display-only details. `null` for
 * every other decision. A gain panel needs a way to send many tokens at once, so it is offered
 * only when `canBatch`; a redistribution is a single decision and always is.
 */
export function describeCommandTokens(
  choice: Pick<PendingChoiceDto, "options" | "details">,
  canBatch: boolean,
  resourcesOf?: ReadonlyMap<string, number>,
): CommandTokenView | null {
  const view = describeBase(choice, canBatch, resourcesOf);
  if (!view) return null;
  const pools = choice.details?.pools as Partial<CommandPools> | undefined;
  return { ...view, fleetSupplyBonus: asCount(pools?.fleet_supply_bonus) ?? 0 };
}

function describeBase(
  choice: Pick<PendingChoiceDto, "options" | "details">,
  canBatch: boolean,
  /** Resource value per planet id (from the board), so the flow can show and weigh resources. */
  resourcesOf?: ReadonlyMap<string, number>,
): Omit<CommandTokenView, "fleetSupplyBonus"> | null {
  const details = choice.details;
  if (!details) return null;
  // Leadership's secondary window keeps its strategy-secondary details and adds the purchase.
  const purchase = describePurchase(details.purchase, resourcesOf);
  const buyWindow = details.kind === "strategy_secondary" && purchase !== null;
  if (details.kind !== "command_tokens" && !buyWindow) return null;
  const pools = details.pools as Partial<Record<TokenPool, unknown>> | undefined;
  const current = {
    tactic: asCount(pools?.tactic),
    fleet: asCount(pools?.fleet),
    strategic: asCount(pools?.strategic),
  };
  if (current.tactic === null || current.fleet === null || current.strategic === null) return null;
  const held: Pools = { tactic: current.tactic, fleet: current.fleet, strategic: current.strategic };
  const reinforcements = asCount(details.reinforcements);
  if (details.mode === "buy") {
    // The yes/no purchase question itself: nothing free to place, only what can be bought.
    const ids = new Set(choice.options.map((option) => option.id));
    if (!canBatch || !purchase || !ids.has("yes") || !ids.has("no")) return null;
    return { mode: "gain", current: held, reinforcements, total: 0, gained: 0, arrangements: new Set(), purchase };
  }
  if (details.mode === "gain") {
    const total = asCount(details.tokens_to_place);
    const ids = new Set(choice.options.map((option) => option.id));
    if (!canBatch || !total || !TOKEN_POOLS.every((pool) => ids.has(gainOptionId(pool)))) {
      return null;
    }
    return { mode: "gain", current: held, reinforcements, total, gained: 0, arrangements: new Set(), purchase };
  }
  if (details.mode === "restack") {
    const total = asCount(details.total);
    const moves = new Set(
      choice.options.filter((option) => option.kind === "redistribute").map((option) => option.id),
    );
    if (!total || moves.size === 0) return null;
    return {
      mode: "restack",
      current: held,
      reinforcements,
      total,
      gained: 0,
      arrangements: moves,
      purchase: null,
    };
  }
  if (details.mode === "redistribute") {
    const total = asCount(details.total);
    const arrangements = new Set(
      choice.options.filter((option) => option.kind === "redistribute").map((option) => option.id),
    );
    if (total === null || arrangements.size === 0) return null;
    return {
      mode: "redistribute",
      current: held,
      reinforcements,
      total,
      gained: asCount(details.gained) ?? 0,
      arrangements,
      purchase: null,
    };
  }
  return null;
}

function describePurchase(raw: unknown, resourcesOf?: ReadonlyMap<string, number>): PurchaseView | null {
  if (!raw || typeof raw !== "object") return null;
  const source = raw as Record<string, unknown>;
  const cost = asCount(source.cost);
  const influence = asCount(source.influence_available);
  const max = asCount(source.max);
  const tradeGoods = asCount(source.trade_goods);
  const tradeGoodWorth = asCount(source.trade_good_worth);
  if (!cost || influence === null || !max || tradeGoods === null || !tradeGoodWorth) return null;
  const planets: PurchasePlanet[] = [];
  for (const entry of Array.isArray(source.planets) ? source.planets : []) {
    const planet = entry as Record<string, unknown>;
    const worth = asCount(planet?.worth);
    if (typeof planet?.id !== "string" || worth === null) return null;
    planets.push({ id: planet.id, worth, resources: resourcesOf?.get(planet.id) ?? null });
  }
  return { cost, influence, max, planets, tradeGoods, tradeGoodWorth };
}

export const stagedTotal = (staging: TokenStaging): number =>
  TOKEN_POOLS.reduce((sum, pool) => sum + staging[pool], 0);

/** The staging before the player touches anything. */
export function initialStaging(view: CommandTokenView): TokenStaging {
  return view.mode === "gain"
    ? { tactic: 0, fleet: 0, strategic: 0 }
    : { ...view.current };
}

/** Every token to assign: the free ones plus the ones bought with influence. */
export const tokensToAssign = (view: CommandTokenView, bought = 0): number => view.total + bought;

/** Tokens still to assign. */
export const tokensRemaining = (
  view: CommandTokenView,
  staging: TokenStaging,
  bought = 0,
): number => Math.max(0, tokensToAssign(view, bought) - stagedTotal(staging));

export const canAddToken = (view: CommandTokenView, staging: TokenStaging, bought = 0): boolean =>
  tokensRemaining(view, staging, bought) > 0;

export const canRemoveToken = (staging: TokenStaging, pool: TokenPool): boolean =>
  staging[pool] > 0;

export function addToken(
  view: CommandTokenView,
  staging: TokenStaging,
  pool: TokenPool,
  bought = 0,
): TokenStaging {
  return canAddToken(view, staging, bought) ? { ...staging, [pool]: staging[pool] + 1 } : staging;
}

export function removeToken(staging: TokenStaging, pool: TokenPool): TokenStaging {
  return canRemoveToken(staging, pool) ? { ...staging, [pool]: staging[pool] - 1 } : staging;
}

/** What the pool will hold once the staging is confirmed. */
export const resultingCount = (
  view: CommandTokenView,
  staging: TokenStaging,
  pool: TokenPool,
): number => (view.mode === "gain" ? view.current[pool] + staging[pool] : staging[pool]);

export interface PoolPips {
  /** Tokens that stay as they are. */
  kept: number;
  /** Newly assigned tokens. */
  added: number;
  /** Tokens that were here and are taken out (redistribute only). */
  removed: number;
}

export function poolPips(view: CommandTokenView, staging: TokenStaging, pool: TokenPool): PoolPips {
  if (view.mode === "gain") {
    return { kept: view.current[pool], added: staging[pool], removed: 0 };
  }
  const now = view.current[pool];
  const next = staging[pool];
  return {
    kept: Math.min(now, next),
    added: Math.max(0, next - now),
    removed: Math.max(0, now - next),
  };
}

/** The option id of the redistribution that matches the staging, when the engine offers it. */
export function arrangementId(view: CommandTokenView, staging: TokenStaging): string | null {
  const id = `${staging.tactic}|${staging.fleet}|${staging.strategic}`;
  return view.mode === "redistribute" && view.arrangements.has(id) ? id : null;
}

/** Why a fully assigned staging still cannot be confirmed, or `null` when it can. */
export function confirmBlocker(
  view: CommandTokenView,
  staging: TokenStaging,
  bought = 0,
): string | null {
  const remaining = tokensRemaining(view, staging, bought);
  if (remaining > 0) {
    return `Assign ${remaining} more token${remaining === 1 ? "" : "s"} to confirm.`;
  }
  if (view.mode === "redistribute" && arrangementId(view, staging) === null) {
    const floor = minFleetOffered(view);
    return floor > 0 && staging.fleet < floor
      ? `Keep at least ${floor} tokens in the fleet pool.`
      : "That arrangement is not allowed.";
  }
  return null;
}

/** A purchase question can be answered "no", so a view with a purchase confirms even at 0 tokens. */
export const canConfirmTokens = (
  view: CommandTokenView,
  staging: TokenStaging,
  bought = 0,
  override: PaymentOverride | null = null,
): boolean =>
  (tokensToAssign(view, bought) > 0 || view.purchase !== null) &&
  confirmBlocker(view, staging, bought) === null &&
  (view.purchase === null || paymentCheck(view, bought, override).problem === null);

/** A step of a token batch plan, as the server's `tokens` batch kind takes it. */
export type TokenStep =
  | { kind: "pool"; pool: string }
  /** Answer a "spend 3 influence for a command token" question. */
  | { kind: "purchase"; buy: boolean }
  /** Part of a purchase's payment: exhaust a planet, or spend a trade good. */
  | { kind: "exhaust"; planet: string }
  | { kind: "trade_good" };

/**
 * How many tokens can be bought: what the influence pays for, what the listed planets and trade
 * goods can actually be spent as, and what the reinforcements can still hold after the free ones.
 */
export function maxPurchases(view: CommandTokenView): number {
  const purchase = view.purchase;
  if (!purchase) return 0;
  const spendable =
    purchase.planets.reduce((sum, planet) => sum + planet.worth, 0) +
    purchase.tradeGoods * purchase.tradeGoodWorth;
  const room =
    view.reinforcements === null ? Infinity : Math.max(0, view.reinforcements - view.total);
  return Math.max(0, Math.min(purchase.max, Math.floor(spendable / purchase.cost), room));
}

/** Takes tokens back out of the pools (strategy first) until the staging fits the total. */
export function fitStaging(view: CommandTokenView, staging: TokenStaging, bought: number): TokenStaging {
  const fitted = { ...staging };
  const limit = tokensToAssign(view, bought);
  for (const pool of [...TOKEN_POOLS].reverse()) {
    while (stagedTotal(fitted) > limit && fitted[pool] > 0) fitted[pool] -= 1;
  }
  return fitted;
}

/** What paying for `bought` tokens spends. */
export interface PaymentPlan {
  planets: PurchasePlanet[];
  tradeGoods: number;
  /** Influence the spent planets and goods are worth. */
  spent: number;
  /** What the tokens cost. */
  bill: number;
  /** Worth beyond the bill; it carries over to the next token and is lost after the last. */
  extra: number;
}

/**
 * The payment for `bought` tokens, chosen with the payment drawer's Auto-pay rule: the planets
 * covering the bill with the least waste and then the fewest planets, trade goods only when the
 * planets alone fall short. The engine asks its payment questions without any automatic rule, so
 * the plan names each exhaust and trade good explicitly. `null` when it cannot be covered.
 */
export function planPayment(view: CommandTokenView, bought: number): PaymentPlan | null {
  const purchase = view.purchase;
  if (!purchase) return null;
  const bill = purchase.cost * bought;
  if (bought === 0) return { planets: [], tradeGoods: 0, spent: 0, bill: 0, extra: 0 };
  const planets: PayablePlanet[] = purchase.planets.map((planet) => ({
    id: `exhaust|${planet.id}`,
    planetId: planet.id,
    planetName: planet.id,
    worth: planet.worth,
    label: planet.id,
    resources: planet.resources ?? undefined,
  }));
  const offer: PaymentOffer = {
    planets,
    hasTradeGoodOption: purchase.tradeGoods > 0,
    tradeGoodWorth: purchase.tradeGoodWorth,
    owed: bill,
    totalAmount: bill,
    alreadyPaid: 0,
    currency: "Influence",
  };
  const pick = suggestAutoPay(offer, purchase.tradeGoods);
  if (!pick.settled) return null;
  const chosen = pick.planetIds
    .map((id) => purchase.planets.find((planet) => `exhaust|${planet.id}` === id))
    .filter((planet): planet is PurchasePlanet => planet !== undefined);
  const spent =
    chosen.reduce((sum, planet) => sum + planet.worth, 0) + pick.tradeGoods * purchase.tradeGoodWorth;
  return { planets: chosen, tradeGoods: pick.tradeGoods, spent, bill, extra: spent - bill };
}

/** A payment the player chose instead of Auto-pay: which planets to exhaust, how many goods to spend. */
export interface PaymentOverride {
  planetIds: string[];
  tradeGoods: number;
}

/** The running account of a payment against the bill of the staged purchases. */
export interface PaymentCheck {
  plan: PaymentPlan | null;
  bill: number;
  paid: number;
  /** Still to pay (0 when covered). */
  remainder: number;
  /** Influence beyond the bill that the last item spent; carried to the next token or lost. */
  waste: number;
  /** Why the engine would not take this payment, or `null` when it would. */
  problem: string | null;
}

/** The Auto-pay payment as an override, the starting point for editing it. */
export function overrideFromPlan(plan: PaymentPlan): PaymentOverride {
  return { planetIds: plan.planets.map((planet) => planet.id), tradeGoods: plan.tradeGoods };
}

/** The payment an override names, in the listed planets' order. */
function overridePlan(view: CommandTokenView, bought: number, override: PaymentOverride): PaymentPlan | null {
  const purchase = view.purchase;
  if (!purchase) return null;
  const planets = purchase.planets.filter((planet) => override.planetIds.includes(planet.id));
  const tradeGoods = Math.max(0, Math.min(purchase.tradeGoods, Math.floor(override.tradeGoods)));
  const spent =
    planets.reduce((sum, planet) => sum + planet.worth, 0) + tradeGoods * purchase.tradeGoodWorth;
  const bill = purchase.cost * bought;
  return { planets, tradeGoods, spent, bill, extra: spent - bill };
}

/**
 * Whether the engine takes a payment, and the account. Its payment loop asks for one planet or
 * trade good at a time and stops the moment the bill is covered, so the last item may overshoot
 * (the surplus is carried to the next bought token, or lost after the last) but an item the bill
 * did not need is not offered: such a plan is illegal. Planets the engine would strand are not an
 * issue while the listed planets and goods cover the bill. With no override this is Auto-pay.
 */
export function paymentCheck(
  view: CommandTokenView,
  bought: number,
  override: PaymentOverride | null,
): PaymentCheck {
  const purchase = view.purchase;
  const bill = purchase ? purchase.cost * bought : 0;
  const plan = override ? overridePlan(view, bought, override) : planPayment(view, bought);
  if (!purchase || !plan) {
    return { plan: null, bill, paid: 0, remainder: bill, waste: 0, problem: "The bill cannot be covered." };
  }
  const paid = plan.spent;
  const remainder = Math.max(0, bill - paid);
  const waste = Math.max(0, paid - bill);
  if (bought === 0) {
    const need = Math.max(0, purchase.cost - paid);
    return {
      plan,
      bill,
      paid,
      remainder: 0,
      waste: paid,
      problem:
        paid > 0
          ? `Selected ${paid} influence buys no token: select ${need} more influence, or clear the selection.`
          : null,
    };
  }
  let problem: string | null = null;
  if (paid < bill) {
    problem = `Short by ${bill - paid} influence: exhaust another planet or spend more trade goods.`;
  } else {
    const items = [
      ...plan.planets.map((planet) => ({ name: planet.id, worth: planet.worth })),
      ...(plan.tradeGoods > 0 ? [{ name: "a trade good", worth: purchase.tradeGoodWorth }] : []),
    ];
    const smallest = items.reduce((a, b) => (b.worth < a.worth ? b : a));
    if (paid - smallest.worth >= bill) {
      problem = `More than needed: the bill is ${bill} and this pays ${paid}. The engine stops asking once the bill is covered, so take out ${smallest.name} (${smallest.worth}).`;
    }
  }
  return { plan, bill, paid, remainder, waste, problem };
}

/** Whether an override is still a legal payment for `bought` tokens. */
export const overrideIsValid = (
  view: CommandTokenView,
  bought: number,
  override: PaymentOverride,
): boolean => paymentCheck(view, bought, override).problem === null;

/** The gain as one plan: a pool step per token, tactic first, then fleet, then strategy. */
export function tokenPlan(staging: TokenStaging): TokenStep[] {
  return TOKEN_POOLS.flatMap((pool) =>
    Array.from({ length: staging[pool] }, () => ({ kind: "pool" as const, pool: gainOptionId(pool) })),
  );
}

/**
 * The gain and the purchases as one plan, in the order the engine asks: the free tokens' pools,
 * then per purchase the yes, its payment (until the running bill is covered; overpayment carries
 * to the next token) and its pool, then the closing "no" the engine asks while more is affordable.
 * `null` when the payment cannot be covered.
 */
export function tokenPlanWithPurchase(
  view: CommandTokenView,
  staging: TokenStaging,
  bought: number,
  override: PaymentOverride | null = null,
): TokenStep[] | null {
  const purchase = view.purchase;
  if (!purchase) return tokenPlan(staging);
  const check = paymentCheck(view, bought, override);
  const plan = check.plan;
  if (!plan || (bought > 0 && check.problem !== null)) return null;
  const pools = tokenPlan(staging);
  const steps: TokenStep[] = pools.slice(0, view.total);
  const items: { step: TokenStep; worth: number }[] = [
    ...plan.planets.map((planet) => ({
      step: { kind: "exhaust" as const, planet: planet.id },
      worth: planet.worth,
    })),
    ...Array.from({ length: plan.tradeGoods }, () => ({
      step: { kind: "trade_good" as const },
      worth: purchase.tradeGoodWorth,
    })),
  ];
  // A chosen payment goes in by worth, smallest last, so the last item is the one that may overshoot.
  if (override) items.sort((a, b) => b.worth - a.worth);
  let covered = 0;
  let next = 0;
  for (let token = 1; token <= bought; token += 1) {
    steps.push({ kind: "purchase", buy: true });
    while (covered < purchase.cost * token && next < items.length) {
      steps.push(items[next].step);
      covered += items[next].worth;
      next += 1;
    }
    steps.push(pools[view.total + token - 1]);
  }
  // The engine asks again exactly while the influence still pays for one more.
  if (bought < purchase.max) steps.push({ kind: "purchase", buy: false });
  return steps;
}

export type TokenOutcome =
  | { kind: "plan"; steps: TokenStep[] }
  | { kind: "option"; optionId: string }
  /** Restack: one engine question per move, then "done" when the engine would ask again. */
  | { kind: "moves"; moveIds: string[]; finish: boolean };

/** The engine's id for a pool in a restack move (`source|destination`). */
const restackPool = (pool: TokenPool): string => (pool === "strategic" ? "strategy" : pool);

/**
 * The fewest single-token moves that turn the pools into `staging`, as the engine's `from|to`
 * ids. Sources are always pools with a surplus, so each move is offered when its turn comes.
 */
export function restackMoves(view: CommandTokenView, staging: TokenStaging): string[] {
  const surplus = TOKEN_POOLS.flatMap((pool) =>
    Array.from({ length: Math.max(0, view.current[pool] - staging[pool]) }, () => pool),
  );
  const deficit = TOKEN_POOLS.flatMap((pool) =>
    Array.from({ length: Math.max(0, staging[pool] - view.current[pool]) }, () => pool),
  );
  return surplus.map((from, i) => `${restackPool(from)}|${restackPool(deficit[i])}`);
}

/** What confirming sends: a batch plan for a gain, the arrangement's option for a redistribute. */
export function tokenOutcome(
  view: CommandTokenView,
  staging: TokenStaging,
  bought = 0,
  override: PaymentOverride | null = null,
): TokenOutcome | null {
  if (!canConfirmTokens(view, staging, bought, override)) return null;
  if (view.mode === "gain") {
    const steps = tokenPlanWithPurchase(view, staging, bought, override);
    return steps ? { kind: "plan", steps } : null;
  }
  if (view.mode === "restack") {
    const moveIds = restackMoves(view, staging);
    // The engine stops asking by itself once as many moves as tokens were made.
    return { kind: "moves", moveIds, finish: moveIds.length < view.total };
  }
  const optionId = arrangementId(view, staging);
  return optionId === null ? null : { kind: "option", optionId };
}

/** The purchase's bill as a payment offer, so the map and the shared bar can reuse the payment machinery. */
export function purchaseOffer(view: CommandTokenView, bought: number): PaymentOffer | null {
  const purchase = view.purchase;
  if (!purchase) return null;
  const bill = purchase.cost * bought;
  return {
    planets: purchase.planets.map((planet) => ({
      id: `exhaust|${planet.id}`,
      planetId: planet.id,
      planetName: planet.id,
      worth: planet.worth,
      label: planet.id,
      resources: planet.resources ?? undefined,
    })),
    hasTradeGoodOption: purchase.tradeGoods > 0,
    tradeGoodWorth: purchase.tradeGoodWorth,
    owed: bill,
    totalAmount: bill,
    alreadyPaid: 0,
    currency: "Influence",
  };
}

/** What would be paid now: the player's own choice, else the Auto-pay plan (empty when none fits). */
export function effectivePayment(
  view: CommandTokenView,
  bought: number,
  override: PaymentOverride | null,
): PaymentOverride {
  if (override) return override;
  const plan = planPayment(view, bought);
  return plan ? overrideFromPlan(plan) : { planetIds: [], tradeGoods: 0 };
}

/** The effective payment in the payment draft's terms (option ids), for the map's marks. */
export function paymentDraftOf(
  view: CommandTokenView,
  bought: number,
  override: PaymentOverride | null,
): PaymentDraft {
  const pay = effectivePayment(view, bought, override);
  return { planetIds: pay.planetIds.map((id) => `exhaust|${id}`), tradeGoods: pay.tradeGoods };
}

/**
 * What the player has staged for the purchase, planet first: the payment (planets and trade goods)
 * decides how many tokens are bought. `override` null means "the suggestion for `bought` tokens"
 * (nothing at all when `bought` is 0).
 */
export interface PurchaseState {
  bought: number;
  override: PaymentOverride | null;
}

export const NO_PURCHASE: PurchaseState = { bought: 0, override: null };

/** Influence a payment is worth (planets plus trade goods). */
export function paymentInfluence(view: CommandTokenView, pay: PaymentOverride): number {
  const purchase = view.purchase;
  if (!purchase) return 0;
  const planets = purchase.planets
    .filter((planet) => pay.planetIds.includes(planet.id))
    .reduce((sum, planet) => sum + planet.worth, 0);
  const goods = Math.max(0, Math.min(purchase.tradeGoods, Math.floor(pay.tradeGoods)));
  return planets + goods * purchase.tradeGoodWorth;
}

/** Tokens a payment buys: whole `cost`s of influence, at most what the engine allows. */
export function purchasesFor(view: CommandTokenView, pay: PaymentOverride): number {
  const purchase = view.purchase;
  if (!purchase) return 0;
  return Math.min(Math.floor(paymentInfluence(view, pay) / purchase.cost), maxPurchases(view));
}

/** A chosen payment as purchase state: the token count follows it; an empty payment is no purchase. */
export function purchaseFromPayment(view: CommandTokenView, pay: PaymentOverride): PurchaseState {
  if (pay.planetIds.length === 0 && pay.tradeGoods === 0) return NO_PURCHASE;
  return { bought: purchasesFor(view, pay), override: pay };
}

/** The payment staged right now: the player's own selection, else the suggestion for `bought` tokens. */
export function stagedPayment(view: CommandTokenView, state: PurchaseState): PaymentOverride {
  return effectivePayment(view, state.bought, state.override);
}

/** A click on a planet (map or list): flips it in the staged payment; the token count follows. */
export function togglePaymentPlanet(
  view: CommandTokenView,
  state: PurchaseState,
  planetId: string,
): PurchaseState {
  const purchase = view.purchase;
  if (!purchase || !purchase.planets.some((planet) => planet.id === planetId)) return state;
  const base = stagedPayment(view, state);
  const has = base.planetIds.includes(planetId);
  return purchaseFromPayment(view, {
    ...base,
    planetIds: has ? base.planetIds.filter((id) => id !== planetId) : [...base.planetIds, planetId],
  });
}

/** Spend one trade good more or less; the token count follows. */
export function stepPaymentGoods(view: CommandTokenView, state: PurchaseState, delta: number): PurchaseState {
  const purchase = view.purchase;
  if (!purchase) return state;
  const base = stagedPayment(view, state);
  return purchaseFromPayment(view, {
    ...base,
    tradeGoods: Math.max(0, Math.min(purchase.tradeGoods, base.tradeGoods + delta)),
  });
}

/** "Buy N tokens": pre-stages the suggested payment (see Auto-pay) for exactly N. */
export function chooseTokenCount(view: CommandTokenView, count: number): PurchaseState {
  const clamped = Math.max(0, Math.min(maxPurchases(view), Math.floor(count)));
  return { bought: clamped, override: null };
}

/**
 * The one-tap suggestion: the suggested planets for the tokens already bought, or for the most the
 * seat can buy when nothing is staged.
 */
export function suggestedPurchase(view: CommandTokenView, state: PurchaseState): PurchaseState {
  return chooseTokenCount(view, state.bought > 0 ? state.bought : maxPurchases(view));
}

export interface PurchaseSummary {
  /** Influence staged (planets plus trade goods). */
  influence: number;
  bought: number;
  cost: number;
  /** Influence staged beyond what the bought tokens cost (lost; no carry beyond the last token). */
  wasted: number;
  /** Resources the exhausted planets would have produced (unknown planets count 0). */
  resourcesLost: number;
  /** More whole tokens' worth is staged than the engine allows (reinforcements or influence limits). */
  atLimit: boolean;
}

export function purchaseSummary(view: CommandTokenView, state: PurchaseState): PurchaseSummary {
  const purchase = view.purchase;
  const pay = stagedPayment(view, state);
  const influence = paymentInfluence(view, pay);
  const cost = purchase?.cost ?? 0;
  const resourcesLost = (purchase?.planets ?? [])
    .filter((planet) => pay.planetIds.includes(planet.id))
    .reduce((sum, planet) => sum + (planet.resources ?? 0), 0);
  return {
    influence,
    bought: state.bought,
    cost,
    wasted: Math.max(0, influence - cost * state.bought),
    resourcesLost,
    atLimit: cost > 0 && Math.floor(influence / cost) > maxPurchases(view),
  };
}

/** The engine's display-only `fleet_supply` detail: ships counted, the enforced limit, Armada's part of it. */
export interface FleetSupplyDetail {
  used: number;
  limit: number;
  /** Ships Armada adds, already inside `limit` (Letnev only). */
  fleetSupplyBonus: number;
}

/** The engine's typed `FleetSupplyHints` of a movement or production choice. */
export function readFleetSupply(raw: unknown): FleetSupplyDetail | null {
  if (!raw || typeof raw !== "object") return null;
  const value = raw as Partial<Record<keyof FleetSupplyHints, unknown>>;
  const used = asCount(value.used);
  const limit = asCount(value.limit);
  if (used === null || limit === null) return null;
  return { used, limit, fleetSupplyBonus: asCount(value.fleet_supply_bonus) ?? 0 };
}
