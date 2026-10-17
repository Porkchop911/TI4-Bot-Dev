import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findStrategyCardMeta } from "../protocol/contentCatalog.ts";

type Chooser = Pick<PendingChoiceDto, "context" | "details">;

const subtypeOf = (choice: Pick<PendingChoiceDto, "context">) => choice.context?.subtype ?? "";

export type StrategyCardPickKind = "disgrace" | "overrule" | "strategize";

export interface StrategyCardPick {
  kind: StrategyCardPickKind;
  /** "Public Disgrace", "Overrule" or "Strategize". */
  cardName: string;
  /** What the card does with the card that is picked. */
  summary: string;
  /** Which printed half of the strategy card is performed (Overrule, Strategize); null for Disgrace. */
  half: "primary" | "secondary" | null;
  /** Public Disgrace: the seat that played it and the card it took away, from the engine. */
  playedBy: string | null;
  takenCard: string | null;
}

const text = (value: unknown): string | null => (typeof value === "string" && value ? value : null);

/** The action card behind a strategy-card pick; `null` for any other decision (including the draft). */
export function describeStrategyCardPick(choice: Chooser): StrategyCardPick | null {
  const subtype = subtypeOf(choice);
  if (subtype === "public_disgrace_choose_card") {
    const first = text(choice.details?.first);
    return {
      kind: "disgrace",
      cardName: "Public Disgrace",
      summary:
        "Your first strategy card was put back on the mat. Choose a different one instead; the card you gave up cannot be taken again.",
      half: null,
      playedBy: text(choice.details?.played_by),
      takenCard: first ? (findStrategyCardMeta(first)?.name ?? first) : null,
    };
  }
  if (subtype === "overrule_pick_strategy card") {
    return {
      kind: "overrule",
      cardName: "Overrule",
      summary:
        "Perform the primary ability of a readied or unchosen strategy card. The card stays where it is; you resolve its primary as the card player.",
      half: "primary",
      playedBy: null,
      takenCard: null,
    };
  }
  if (/^strategize\d*_pick_strategy card$/.test(subtype)) {
    return {
      kind: "strategize",
      cardName: "Strategize",
      summary:
        "Perform the secondary ability of any readied or unchosen strategy card. The card stays where it is.",
      half: "secondary",
      playedBy: null,
      takenCard: null,
    };
  }
  return null;
}

/**
 * Who holds the card an option stands for, from `details.holders` (card id to seat, or null for a
 * card nobody chose). `undefined` when the engine sent no holders (Public Disgrace, the draft).
 */
export function strategyCardHolder(choice: Chooser, option: Pick<ChoiceOptionDto, "id">): string | null | undefined {
  const holders = choice.details?.holders;
  if (!holders || typeof holders !== "object" || Array.isArray(holders)) return undefined;
  const holder = (holders as Record<string, unknown>)[option.id];
  if (holder === undefined) return undefined;
  return typeof holder === "string" ? holder : null;
}
