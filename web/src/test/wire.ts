import type { CombatView, DecisionContext, HistoryStatus } from "../protocol/types.ts";

/**
 * Part of a wire value: the fields that the code under test reads. The wire types are generated
 * from Rust and require every field that the server sends; test data and the dev galleries name
 * only what matters to them, and these say so.
 */
export type Loose<T> = T extends readonly (infer U)[]
  ? Loose<U>[]
  : T extends object
    ? { [K in keyof T]?: Loose<T[K]> }
    : T;

/** A decision context of test data: usually its `subtype`, a target or the outstanding amounts. */
export const partialContext = (context: Loose<DecisionContext>): DecisionContext =>
  context as DecisionContext;

/** A space combat of test data. */
export const partialCombat = (combat: Loose<CombatView>): CombatView => combat as CombatView;

/** A history status of test data, as servers sent it before it had a generation. */
export const partialHistory = (history: Loose<HistoryStatus>): HistoryStatus =>
  history as HistoryStatus;
