import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";

/** A faction ability offered for a price: the card as printed, what it costs, and the two answers. */
export interface AbilityOfferView {
  name: string;
  /** When it can be used, e.g. "At the start of each round of space combat". */
  window: string | null;
  /** What it does, as printed. */
  effect: string | null;
  tradeGoods: number;
  held: number;
  yes: ChoiceOptionDto;
  no: ChoiceOptionDto;
}

const isNo = (option: ChoiceOptionDto) =>
  option.kind === "decline" || option.id === "decline" || option.id === "no";

const text = (value: unknown): string | null =>
  typeof value === "string" && value.trim() ? value.trim() : null;

const count = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : null;

/**
 * An optional, paid faction ability (Letnev's Munitions Reserves) as a named offer. `null` for any
 * other decision, or when the options are not a yes/no pair.
 */
export function describeAbilityOffer(
  choice: Pick<PendingChoiceDto, "options" | "details">,
): AbilityOfferView | null {
  const details = choice.details;
  if (!details || details.kind !== "ability_offer") return null;
  const ability = details.ability as Record<string, unknown> | undefined;
  const cost = details.cost as Record<string, unknown> | undefined;
  const name = text(ability?.name);
  const tradeGoods = count(cost?.trade_goods);
  const held = count(cost?.have);
  const no = choice.options.find(isNo);
  const yes = choice.options.find((option) => option !== no);
  if (!name || tradeGoods === null || held === null || !no || !yes || choice.options.length !== 2) {
    return null;
  }
  return {
    name,
    window: text(ability?.window),
    effect: text(ability?.effect),
    tradeGoods,
    held,
    yes,
    no,
  };
}

/** "Spend 2 trade goods (5 → 3) to use Munitions Reserves". */
export function abilityOfferLabel(view: AbilityOfferView): string {
  const unit = view.tradeGoods === 1 ? "trade good" : "trade goods";
  return `Spend ${view.tradeGoods} ${unit} (${view.held} → ${view.held - view.tradeGoods}) to use ${view.name}`;
}
