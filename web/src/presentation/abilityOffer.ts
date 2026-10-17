import type { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";

/** A faction ability offered for a price: the card as printed, what it costs, and the two answers. */
export interface AbilityOfferView {
  name: string;
  /** When it can be used, e.g. "At the start of each round of space combat". */
  window: string | null;
  /** What it does, as printed. */
  effect: string | null;
  tradeGoods: number;
  held: number;
  yes: ChoiceOption;
  /** A second way to say yes (Munitions Reserves: pick the dice to re-roll one by one). */
  alternative: ChoiceOption | null;
  no: ChoiceOption;
  /** The dice just rolled, when the ability acts on them (Munitions Reserves after the roll). */
  dice: AbilityOfferDie[];
}

export interface AbilityOfferDie {
  unit: string;
  face: number;
  hitsOn: number | null;
  hit: boolean | null;
}

const isNo = (option: ChoiceOption) =>
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
  const alternative = choice.options.find((option) => option !== no && option !== yes) ?? null;
  if (
    !name ||
    tradeGoods === null ||
    held === null ||
    !no ||
    !yes ||
    choice.options.length !== (alternative ? 3 : 2)
  ) {
    return null;
  }
  const dice: AbilityOfferDie[] = [];
  if (Array.isArray(details.dice)) {
    for (const entry of details.dice) {
      const record = (entry ?? {}) as Record<string, unknown>;
      const face = typeof record.face === "number" ? record.face : null;
      if (face === null) continue;
      const bonus = typeof record.bonus === "number" ? record.bonus : 0;
      const hitsOn = typeof record.hits_on === "number" ? record.hits_on : null;
      dice.push({
        unit: typeof record.unit === "string" ? record.unit : "",
        face,
        hitsOn,
        hit: hitsOn === null ? null : face + bonus >= hitsOn,
      });
    }
  }
  return {
    name,
    window: text(ability?.window),
    effect: text(ability?.effect),
    tradeGoods,
    held,
    yes,
    alternative,
    no,
    dice,
  };
}

/** "Spend 2 trade goods (5 → 3) to use Munitions Reserves". */
export function abilityOfferLabel(view: AbilityOfferView): string {
  const unit = view.tradeGoods === 1 ? "trade good" : "trade goods";
  const price = `Spend ${view.tradeGoods} ${unit} (${view.held} → ${view.held - view.tradeGoods})`;
  // With the dice in view the button says what it does to them ("reroll all 2 missed dice").
  return view.dice.length > 0 ? `${price}: ${view.yes.label}` : `${price} to use ${view.name}`;
}
