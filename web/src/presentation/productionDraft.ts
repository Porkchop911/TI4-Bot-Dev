import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";

/**
 * The arithmetic of the production builder, shared by the drawer (what a staged draft costs) and by
 * the prepared Warfare secondary (whether a planned build still fits the real question).
 * Option payloads are the engine's (`production.rs::build_options`); nothing here is estimated.
 */

/** Resources a staged draft costs: printed costs less the single one-time discount of the use. */
export function draftResourceCost(options: ChoiceOptionDto[], draft: Record<string, number>): number {
  let printedTotal = 0;
  let discountOnce = 0;
  for (const option of options) {
    const batches = draft[option.id] ?? 0;
    if (!batches) continue;
    if (option.payload?.free_this_use === true || isExchangeOption(option)) continue;
    const cost = option.payload?.cost;
    if (typeof cost !== "number" || !Number.isSafeInteger(cost) || cost < 0) return Infinity;
    const printed = option.payload?.printed_cost;
    const discount = option.payload?.discount;
    if (
      typeof printed === "number" &&
      Number.isSafeInteger(printed) &&
      printed >= cost &&
      typeof discount === "number" &&
      Number.isSafeInteger(discount) &&
      discount >= 0
    ) {
      printedTotal += batches * printed;
      // Every offered option previews the same one-time discount. Count it once, not per batch.
      discountOnce = Math.max(discountOnce, Math.min(discount, printed));
    } else {
      printedTotal += batches * cost;
    }
  }
  return Math.max(0, printedTotal - discountOnce);
}

export interface ProductionTotalLine {
  optionId: string;
  unit: string;
  /** Batches of this option staged. */
  batches: number;
  /** Printed resources for all of those batches (0 when free or an exchange). */
  printed: number;
  exchange: boolean;
}

export interface ProductionTotal {
  /** Staged builds (batches). */
  builds: number;
  /** Resources the staged builds cost after the one-time discount; null when an option has no usable cost. */
  total: number | null;
  /** Resources before the discount. */
  printed: number;
  /** What the one-time cost reduction (Sarween Tools and the like) takes off. */
  discount: number;
  lines: ProductionTotalLine[];
}

/**
 * The bill of everything staged in the builder, for the total shown on the first dialog. The sum is
 * `draftResourceCost` (the same figure the budget meter and the payment panel use), so exchange
 * builds and free uses cost nothing and the one-time discount counts once.
 */
export function productionTotal(options: ChoiceOptionDto[], draft: Record<string, number>): ProductionTotal {
  const lines: ProductionTotalLine[] = [];
  let builds = 0;
  let printed = 0;
  for (const option of options) {
    const batches = draft[option.id] ?? 0;
    if (!batches) continue;
    builds += batches;
    const exchange = isExchangeOption(option);
    const free = exchange || option.payload?.free_this_use === true;
    const printedCost = option.payload?.printed_cost;
    const cost = option.payload?.cost;
    const each = free
      ? 0
      : typeof printedCost === "number" && typeof cost === "number" && printedCost >= cost
        ? printedCost
        : typeof cost === "number"
          ? cost
          : 0;
    printed += each * batches;
    lines.push({ optionId: option.id, unit: optionUnitType(option), batches, printed: each * batches, exchange });
  }
  const resourceCost = draftResourceCost(options, draft);
  const total = Number.isFinite(resourceCost) ? resourceCost : null;
  return { builds, total, printed, discount: total === null ? 0 : Math.max(0, printed - total), lines };
}

/** Production capacity one batch of this option uses. */
export const optionCapacity = (option: ChoiceOptionDto): number =>
  typeof option.payload?.production_spent === "number"
    ? option.payload.production_spent
    : typeof option.payload?.placed === "number"
      ? option.payload.placed
      : typeof option.payload?.count === "number"
        ? option.payload.count
        : 1;

/** True for the Amalgamation option: return a captured model of the type instead of paying. */
export const isExchangeOption = (option: ChoiceOptionDto): boolean =>
  option.payload?.exchange === true;

/** The unit type an option makes (its payload, else its id), for both the build and the exchange. */
export const optionUnitType = (option: ChoiceOptionDto): string =>
  String(option.payload?.unit ?? option.id);

/**
 * What names an option in a staged build list. A paid build is its unit type; the exchange of the
 * same type is its own option id (`exchange|carrier`), so the two never share a key and a staged
 * "carrier" can only ever mean the paid one.
 */
export const optionUnit = (option: ChoiceOptionDto): string =>
  isExchangeOption(option) ? option.id : optionUnitType(option);

/** The production batch step for one staged build of this option. */
export const produceStep = (
  option: ChoiceOptionDto,
): { kind: "produce"; unit: string; count: number; exchange?: true } => ({
  kind: "produce",
  unit: optionUnitType(option),
  count: Number(option.payload?.count ?? 1),
  ...(isExchangeOption(option) ? { exchange: true as const } : {}),
});

/** The staged-build key of a batch step (the inverse of `optionUnit` for steps). */
export const stepUnitKey = (step: { unit: string; exchange?: boolean }): string =>
  step.exchange ? `exchange|${step.unit}` : step.unit;

const isBuild = (option: ChoiceOptionDto) => option.id !== "decline" && option.kind !== "decline";

/** The production room the question states: capacity left and resources that can be spent. */
export function productionRoom(choice: PendingChoiceDto): {
  capacityLeft: number;
  resources: number | null;
} {
  const constraint = choice.context?.outstanding?.[0];
  const capacityLeft = Math.max(0, (constraint?.amount ?? 0) - (constraint?.paid ?? 0));
  const options = choice.options.filter(isBuild);
  const available = options.find((o) => typeof o.payload?.available_resources === "number")?.payload
    ?.available_resources;
  const credit = options.find((o) => typeof o.payload?.credit === "number")?.payload?.credit;
  const resources =
    typeof available === "number" && Number.isSafeInteger(available) && available >= 0
      ? available +
        (typeof credit === "number" && Number.isSafeInteger(credit) && credit > 0 ? credit : 0)
      : null;
  return { capacityLeft, resources };
}

/** The system the production question is about. */
export function productionSystem(choice: PendingChoiceDto): string {
  const target = choice.context?.target;
  return target && "System" in target ? String(target.System) : "";
}

export type BuildPlanResult =
  | {
      ok: true;
      destination: string;
      draft: Record<string, number>;
      steps: { unit: string; count: number; exchange?: boolean }[];
    }
  | { ok: false; reason: string };

/**
 * Checks planned builds (unit ids, one batch each, in order) against the real production question
 * the way the drawer's own limits do: every unit must be offered, the batches must fit the capacity
 * left, and the staged cost must fit the resources. Returns the drawer draft (batches per option id)
 * and the production steps to send.
 */
export function planBuilds(choice: PendingChoiceDto, builds: readonly string[]): BuildPlanResult {
  const options = choice.options.filter(isBuild);
  const destination = productionSystem(choice);
  if (!destination) return { ok: false, reason: "The production system is not named by the question." };
  if (!builds.length) return { ok: false, reason: "No units were prepared." };
  const draft: Record<string, number> = {};
  const steps: { unit: string; count: number; exchange?: boolean }[] = [];
  let capacity = 0;
  const { capacityLeft, resources } = productionRoom(choice);
  for (const unit of builds) {
    const matching = options.filter((option) => optionUnit(option) === unit);
    if (matching.length !== 1) {
      return {
        ok: false,
        reason:
          matching.length === 0
            ? `${unit} can no longer be built here.`
            : `${unit} is offered in several ways; choose by hand.`,
      };
    }
    const option = matching[0];
    draft[option.id] = (draft[option.id] ?? 0) + 1;
    capacity += optionCapacity(option);
    steps.push(produceStep(option));
    if (capacity > capacityLeft) {
      return { ok: false, reason: "The prepared builds no longer fit the production capacity." };
    }
    if (resources !== null && draftResourceCost(options, draft) > resources) {
      return { ok: false, reason: "The prepared builds cost more than you can spend now." };
    }
  }
  return { ok: true, destination, draft, steps };
}
