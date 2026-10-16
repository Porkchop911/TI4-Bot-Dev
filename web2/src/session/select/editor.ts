// An editor of the live game: the game asks one question at a time, and an editor stages all
// the questions of one part of a card on one screen, then answers them at once. The registry of
// the editors is `editors.ts`.
import type { BlockView, BoardTaskView, PaymentView, PlayerPickView, RowView } from "../../model";
import type { PlanStep } from "../movementPlan";
import type { Catalog, Choice, SessionUpdate } from "../wire";

/** What the main button sends: the answers by the ids of their options, then a plan that the game takes as it asks. */
export interface EditorAnswers {
  choices: string[];
  plan: PlanStep[] | null;
}

/** What an editor shows, and what its main button sends. */
export interface EditorView {
  blocks: BlockView[];
  /** The map is the choice, or the payment. */
  task?: BoardTaskView | null;
  /** The player table is the choice. */
  pick?: PlayerPickView | null;
  /** The payment bar and the payment line of the footer. */
  payment?: PaymentView | null;
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  /** The main button. */
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** Null while nothing can be sent. */
  send: EditorAnswers | null;
}

/**
 * An editor of the questions whose staging is `D`. Each change returns the staging after it, or
 * null when the question is not one of the editor or the change does nothing.
 */
export interface Editor<D> {
  /** The screen of the question. Null: the question is not one of this editor. */
  view(
    update: SessionUpdate,
    choice: Choice,
    draft: D | null,
    printed: Catalog | null,
  ): EditorView | null;
  /** A system of the map. */
  system?(choice: Choice, draft: D | null, system: string): D | null;
  /** A planet of the map. */
  planet?(choice: Choice, draft: D | null, planet: string): D | null;
  /** A seat of the player table. */
  seat?(choice: Choice, draft: D | null, seat: string): D | null;
  /** A row of a list in the panel. */
  option?(choice: Choice, draft: D | null, group: string | undefined, option: string): D | null;
  /** "Change" of a chosen part. */
  part?(choice: Choice, draft: D | null, part: string): D | null;
  /** Esc: the last choice is taken back. Null: Esc clears the whole screen. */
  last?(choice: Choice, draft: D | null): D | null;
  /** A counter of the panel. */
  count?(choice: Choice, draft: D | null, key: string, value: number): D | null;
  /** The trade goods of the payment, or its commodities (`source`, the id of the counter). */
  goods?(choice: Choice, draft: D | null, value: number, source: string): D | null;
  /** Auto-pay, or "Clear payment". */
  pay?(choice: Choice, draft: D | null, how: "auto" | "reset"): D | null;
}

/** What the player staged in the editor of the open question. */
export interface EditorSlot {
  /** The editor that staged it. */
  id: string;
  draft: unknown;
}

/** The view of an editor that answers by the ids of options alone (`answers`, in the order that the game asks). */
export const answered = <V extends { answers: string[] | null }>(view: V | null) =>
  view && { ...view, send: view.answers && { choices: view.answers, plan: null } };

/** A number of the details of a question, which the engine sends as JSON. Null: not a number. */
export const num = (value: unknown): number | null => (typeof value === "number" ? value : null);

/**
 * The look-ahead `details[key]` of a question, as the engine writes it (`Choice::hinted`): `T` is
 * its generated wire type (`PurchaseHints`, `ResearchPaymentHints`). Null when the question has none.
 */
export const hintOf = <T>(choice: Choice, key: string): T | null =>
  (choice.details?.[key] as T | undefined) ?? null;

/** "Change" of a chosen part of an editor: the part is open again (`Editor.part`). */
export const changeControl = (part: string): RowView["control"] => ({
  kind: "button",
  button: { label: "Change", tone: "quiet", intent: { type: "clearPart", part } },
});
