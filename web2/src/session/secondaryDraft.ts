// The private draft of a secondary: what the player answered so far to the questions of its
// secondary of the strategic action that is resolved. A draft is a script, as the draft of a
// tactical action is (`draft.ts`): the engine plays it on a copy of the game each time it or the
// game changes (`Transport.draft` with a scope), so nothing here decides a rule.
//
// It can be made from the moment the card is played (`secondary` of the update says when), and
// it is sent when the game asks the seat, if the player marked it ready.
import { stopText } from "./draft";
import type { Choice, DraftOutcome, DraftStep, PlanScope, SecondaryFacts } from "./wire";

export interface SecondaryDraft {
  /** The strategic action that the draft is of: its round, who played the card, and the card. */
  round: number;
  primary: string;
  card: string;
  /** The answers in their order: the id of the option, and its label as the game offered it. */
  answers: { id: string; label: string }[];
  /** The player marked the draft ready: it is sent when the game asks the seat. */
  ready: boolean;
}

export const newDraft = (round: number, action: SecondaryFacts): SecondaryDraft => ({
  round,
  primary: action.primary,
  card: action.card,
  answers: [],
  ready: false,
});

/** The draft is of this strategic action. */
export const isOf = (draft: SecondaryDraft, round: number, action: SecondaryFacts): boolean =>
  draft.round === round && draft.primary === action.primary && draft.card === action.card;

export const scopeOf = (action: { primary: string; card: string }): PlanScope => ({
  kind: "secondary",
  primary: action.primary,
  card: action.card,
});

/** What the engine is asked to play: the answers by their ids. */
export const scriptOf = (draft: SecondaryDraft | null): DraftStep[] =>
  (draft?.answers ?? []).map(({ id }) => ({ kind: "choose", option_id: id }));

/** One more answer. A draft that changes is not ready. */
export const record = (
  draft: SecondaryDraft,
  option: { id: string; label: string },
): SecondaryDraft => ({ ...draft, answers: [...draft.answers, option], ready: false });

/** The game played every answer of the draft. */
const played = (draft: SecondaryDraft, outcome: DraftOutcome): boolean =>
  outcome.consumed === draft.answers.length;

/**
 * The draft has nothing more to decide: the secondary is over with it, or the run came to
 * something that a draft cannot show (a card that is drawn), and the game goes on from there
 * in Live. Such a draft can be marked ready, and it is what a ready draft must still be when
 * the game asks the seat.
 */
export const isOver = (draft: SecondaryDraft, outcome: DraftOutcome): boolean =>
  played(draft, outcome) && (outcome.stop.kind === "complete" || outcome.stop.kind === "stopped");

/** Why the game did not play the draft, in words. Null when it did. */
export function problemOf(draft: SecondaryDraft | null, outcome: DraftOutcome): string | null {
  const { stop } = outcome;
  switch (stop.kind) {
    case "open":
    case "complete":
      return null;
    case "refused":
      return `An answer of the draft no longer fits. ${stop.reason}`;
    case "stopped":
      return draft && played(draft, outcome) ? null : stopText(stop.reason);
    case "failed":
      return `No draft from here: ${stop.reason}.`;
  }
}

/** Where the draft ends before the secondary does, in words. Null when it does not. */
export const endOf = (outcome: DraftOutcome): string | null =>
  outcome.stop.kind === "stopped" ? stopText(outcome.stop.reason) : null;

/** The first question of the secondary of `action`, as the live game asks it. */
export const asksSecondary = (choice: Choice, action: SecondaryFacts): boolean =>
  choice.details?.kind === "strategy_secondary" && choice.details.card === action.card;

/** The draft as it is kept between two visits (`SeatStore`, slot "secondary"). Null: none, or not a draft. */
export function parseDraft(value: unknown): SecondaryDraft | null {
  const kept = value as Partial<SecondaryDraft> | null;
  return kept && Array.isArray(kept.answers) && typeof kept.card === "string"
    ? (kept as SecondaryDraft)
    : null;
}
