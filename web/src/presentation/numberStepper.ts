import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { formatDecisionSource } from "./decisionSource.ts";

type Counted = Pick<PendingChoiceDto, "context" | "options">;

export interface NumberStepperView {
  cardName: string;
  /** The question, e.g. "how many trade goods to spend". */
  min: number;
  max: number;
  /** The engine option for each count. */
  optionFor: (count: number) => ChoiceOptionDto | undefined;
  /** What `count` buys, in words. */
  describe: (count: number) => string;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** Bribery's "how many trade goods to spend" as a quantity to step, instead of one radio per number. */
export function describeNumberStepper(choice: Counted): NumberStepperView | null {
  if (choice.context?.subtype !== "bribery_pick_count") return null;
  const counts = choice.options
    .filter((option) => option.kind === "count" && /^\d+$/.test(option.id))
    .map((option) => ({ option, n: Number(option.id) }));
  if (counts.length === 0) return null;
  const numbers = counts.map((entry) => entry.n);
  const min = Math.min(...numbers);
  const max = Math.max(...numbers);
  // The steps must be every number from min to max: a gap would make a stepper offer an illegal answer.
  if (numbers.length !== max - min + 1) return null;
  return {
    cardName: formatDecisionSource(choice.context?.source as Record<string, unknown> | undefined) ?? "Bribery",
    min,
    max,
    optionFor: (count) => counts.find((entry) => entry.n === count)?.option,
    describe: (count) =>
      count === 0
        ? "Spend nothing: your vote stays as it is."
        : `Spend ${plural(count, "trade good")}: cast ${plural(count, "additional vote")} for the outcome you voted on.`,
  };
}
