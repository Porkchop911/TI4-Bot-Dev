import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { formatDecisionSource } from "./decisionSource.ts";
import { isDeclineOption } from "./choiceModel.ts";

type Roll = Pick<PendingChoiceDto, "context" | "details" | "options">;

/** A die of the roll being offered for a reroll. */
export interface RerollDie {
  /** Position among all the dice of the roll, in the order the engine asks about them. */
  index: number;
  /** The engine option that rerolls exactly this die (`reroll|<unit>:<die>`). */
  optionId: string;
  unit: string;
  face: number;
  /** Adjustment riding on the die (Crown of Thalnos adds 1). */
  bonus: number;
  /** The result that counts as a hit, or null for a roll that does not hit. */
  hitsOn: number | null;
  /** Whether the die hits now. */
  hit: boolean | null;
  /** The chance a fresh roll of this die hits, 0 to 1 (a d10 against `hitsOn`). */
  chance: number | null;
  /** Already rerolled once by an earlier effect. */
  rerolled: boolean;
  /** Its question has been answered already: the roller can no longer change it. */
  decided: boolean;
}

export interface RerollGroup {
  unit: string;
  hitsOn: number | null;
  dice: RerollDie[];
}

export interface RerollView {
  /** The card or ability offering the reroll ("Fire Team"). */
  source: string | null;
  dice: RerollDie[];
  groups: RerollGroup[];
  /** Index of the die the engine is asking about now. */
  current: number;
}

const num = (value: unknown): number | null => (typeof value === "number" && Number.isFinite(value) ? value : null);

/** The roll to reroll from, or `null` for any decision that is not a reroll question. */
export function describeRerollDice(choice: Roll): RerollView | null {
  if (!choice.options.some((option) => option.kind === "reroll_die")) return null;
  const raw = choice.details?.dice;
  if (!Array.isArray(raw) || raw.length === 0) return null;
  const current = num(choice.details?.current) ?? 0;
  const dice: RerollDie[] = [];
  for (const [index, entry] of raw.entries()) {
    if (!entry || typeof entry !== "object") return null;
    const record = entry as Record<string, unknown>;
    const face = num(record.face);
    const unitIndex = num(record.unit_index);
    const die = num(record.die);
    if (face === null || unitIndex === null || die === null) return null;
    const bonus = num(record.bonus) ?? 0;
    const hitsOn = num(record.hits_on);
    dice.push({
      index,
      optionId: `reroll|${unitIndex}:${die}`,
      unit: typeof record.unit === "string" ? record.unit : "",
      face,
      bonus,
      hitsOn,
      hit: hitsOn === null ? null : face + bonus >= hitsOn,
      chance: hitsOn === null ? null : Math.min(1, Math.max(0, (11 - hitsOn) / 10)),
      rerolled: record.rerolled === true,
      decided: index < current,
    });
  }
  const groups: RerollGroup[] = [];
  for (const die of dice) {
    const last = groups[groups.length - 1];
    if (last && last.unit === die.unit && last.hitsOn === die.hitsOn) last.dice.push(die);
    else groups.push({ unit: die.unit, hitsOn: die.hitsOn, dice: [die] });
  }
  return {
    source: formatDecisionSource(choice.context?.source as Record<string, unknown> | undefined),
    dice,
    groups,
    current,
  };
}

export interface RerollOutcome {
  /** Hits the roll produces as it stands. */
  now: number;
  /** Hits to expect when the ticked dice are redrawn (kept dice count as they are). */
  expected: number;
  /** The most the ticked dice could add or lose: every one hits, or none does. */
  best: number;
  worst: number;
}

/** What rerolling the ticked dice (by `index`) is worth, in hits. A roll that does not hit has none. */
export function rerollOutcome(view: RerollView, ticked: ReadonlySet<number>): RerollOutcome {
  let now = 0;
  let kept = 0;
  let expected = 0;
  let best = 0;
  for (const die of view.dice) {
    if (die.hit === null) continue;
    if (die.hit) now += 1;
    if (ticked.has(die.index)) {
      expected += die.chance ?? 0;
      best += 1;
    } else if (die.hit) {
      kept += 1;
    }
  }
  return {
    now,
    expected: Math.round((kept + expected) * 10) / 10,
    best: kept + best,
    worst: kept,
  };
}

/** The answers that carry out the ticked dice, one per remaining question, in the order asked. */
export function rerollIntents(view: RerollView, ticked: ReadonlySet<number>): Array<(option: ChoiceOptionDto) => boolean> {
  return view.dice
    .filter((die) => !die.decided)
    .map((die) =>
      ticked.has(die.index) ? (option: ChoiceOptionDto) => option.id === die.optionId : (option: ChoiceOptionDto) => isDeclineOption(option),
    );
}
