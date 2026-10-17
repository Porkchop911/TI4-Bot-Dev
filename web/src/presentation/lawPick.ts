import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { humanizeId } from "../protocol/contentCatalog.ts";

type Chooser = Pick<PendingChoiceDto, "context" | "details">;

const LAW_PICK = /^repeal_pick_repeal$/;

/** Repeal Law: which law in play to discard. */
export const isLawPick = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  LAW_PICK.test(choice.context?.subtype ?? "");

export interface LawOption {
  /** The law's printed name, in place of its id ("minister_exploration"). */
  name: string;
  /** "Law" or "Directive". */
  kind: string | null;
  /** What the law was elected onto (a planet id or a seat id), when it names one. */
  elected: string | null;
  /** Which side of a For/Against vote passed it, or null for a law that elects something. */
  passed: "For" | "Against" | null;
  /** The printed text that applies, one paragraph per entry (only the passed side of a For/Against law). */
  text: string[];
}

const str = (value: unknown): string | null => (typeof value === "string" && value.trim() ? value.trim() : null);

/** One law as printed, from `details.laws`; a humanised id when the engine sent no card for it. */
export function describeLawOption(choice: Chooser, option: Pick<ChoiceOptionDto, "id">): LawOption | null {
  if (!isLawPick(choice)) return null;
  const laws = choice.details?.laws;
  const card =
    laws && typeof laws === "object" && !Array.isArray(laws)
      ? ((laws as Record<string, unknown>)[option.id] as Record<string, unknown> | undefined)
      : undefined;
  const outcome = str(card?.outcome);
  const side = outcome && /^(for|against)$/i.test(outcome) ? (outcome.toLowerCase() === "for" ? "For" : "Against") : null;
  const lines = [str(card?.text1), str(card?.text2)].filter((line): line is string => line !== null);
  return {
    name: str(card?.name) ?? humanizeId(option.id),
    kind: str(card?.type),
    elected: side ? null : outcome,
    passed: side,
    // A For/Against law has one effect per side; only the side that passed is in force.
    text: side && lines.length === 2 ? [lines[side === "For" ? 0 : 1]] : lines,
  };
}
