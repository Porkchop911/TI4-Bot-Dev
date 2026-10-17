// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ConstraintKind } from "./ConstraintKind.ts";

/**
 * A quantity that is still owed or still available inside a decision already under way.
 *
 * This is the continuation state a between-prompts board snapshot loses. `paid` is what the seat
 * has already committed within this same transaction, so `remaining` is what a further offer may
 * legitimately ask for; charging `amount` again is the defect this type exists to prevent.
 */
export type OutstandingConstraint = { kind: ConstraintKind, 
/**
 * The full bill or allowance for the transaction.
 */
amount: number, 
/**
 * Already committed within this transaction, including overpayment retained as credit.
 */
paid: number, };
